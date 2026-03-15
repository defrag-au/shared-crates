//! NFT Shape Experiments — exploring alternatives to the card metaphor.
//!
//! Three shape demos rendered with the same IIIF art, rarity border, and stat
//! overlay so they're directly comparable:
//! 1. Square Tile — natural 1:1 fit, art-first
//! 2. Hex Tile — tessellatable, game-board feel
//! 3. Rounded Square — modern app-icon aesthetic
//!
//! Tilt uses proper 3D perspective projection: each vertex is rotated around
//! X/Y axes then projected with perspective division.

use egui::epaint::{Mesh, Vertex};
use egui::text::LayoutJob;
use egui::{Color32, Pos2, Rect, TextFormat, Vec2};
use egui_widgets::icons::{phosphor_family, PhosphorIcon};
use std::f32::consts::TAU;

use crate::{ACCENT, TEXT_MUTED};

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

/// IIIF test image: Pirate758 NFT (1:1 aspect)
const IIIF_ART_URL: &str = "https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465373538/full/400,/0/default.jpg";

use super::card_effects::{
    AuroraCurtain, BrushedMetal, CardEffect, DiffractionGrating, EffectVertex, Glitter,
    PrismaticDispersion, StreakHolo, ThinFilmIridescence, EFFECT_NAMES,
};

// ============================================================================
// State
// ============================================================================

pub struct NftShapesState {
    pub size: f32,
    pub rarity: usize,
    pub tilt_ease: f32,
    pub perspective_distance: f32,
    // Effect selection
    pub effect_index: usize,
    // Streak Holo params
    pub hue_range: f32,
    pub shimmer_width: f32,
    pub shimmer_intensity: f32,
    pub overlay_opacity: f32,
    // Thin Film Iridescence params
    pub iri_min: f32,
    pub iri_range: f32,
    pub fresnel_power: f32,
    pub iri_intensity: f32,
    // Diffraction Grating params
    pub grating_spacing: f32,
    pub grating_angle: f32,
    pub max_orders: u32,
    pub diffraction_intensity: f32,
    // Glitter params
    pub glitter_scale: f32,
    pub sparkle_sharpness: f32,
    pub sparkle_threshold: f32,
    pub glitter_z_depth: f32,
    // Brushed Metal params
    pub roughness_along: f32,
    pub roughness_perp: f32,
    pub brush_angle: f32,
    pub metal_preset: usize, // 0=silver, 1=gold, 2=copper
    // Aurora params
    pub aurora_freq1: f32,
    pub aurora_freq2: f32,
    pub curtain_sharpness: f32,
    pub vertical_falloff: f32,
    pub aurora_brightness: f32,
    // Prismatic params
    pub prism_dispersion: f32,
    pub prism_spread: f32,
    pub facet_scale: f32,
    pub prism_intensity: f32,
    // Spark traversal
    pub spark_phase: f32,
    pub spark_speed: f32,
    pub spark_enabled: bool,
    // Per-demo tilt state
    tilts: [TiltState; 3],
}

#[derive(Clone, Copy, Default)]
struct TiltState {
    current_x: f32,
    current_y: f32,
}

impl Default for NftShapesState {
    fn default() -> Self {
        Self {
            size: 200.0,
            rarity: 2,
            tilt_ease: 0.1,
            perspective_distance: 800.0,
            effect_index: 0,
            hue_range: 60.0,
            shimmer_width: 0.15,
            shimmer_intensity: 0.4,
            overlay_opacity: 0.2,
            iri_min: 250.0,
            iri_range: 400.0,
            fresnel_power: 5.0,
            iri_intensity: 0.3,
            grating_spacing: 1500.0,
            grating_angle: 0.0,
            max_orders: 4,
            diffraction_intensity: 1.5,
            glitter_scale: 40.0,
            sparkle_sharpness: 150.0,
            sparkle_threshold: 0.3,
            glitter_z_depth: 0.6,
            roughness_along: 0.05,
            roughness_perp: 0.8,
            brush_angle: 0.0,
            metal_preset: 0,
            aurora_freq1: 8.0,
            aurora_freq2: 5.0,
            curtain_sharpness: 4.0,
            vertical_falloff: 1.5,
            aurora_brightness: 1.0,
            prism_dispersion: 0.08,
            prism_spread: 0.1,
            facet_scale: 8.0,
            prism_intensity: 1.5,
            spark_phase: 0.0,
            spark_speed: 0.3,
            spark_enabled: true,
            tilts: [TiltState::default(); 3],
        }
    }
}

// ============================================================================
// Rarity
// ============================================================================

const RARITIES: &[(&str, Color32)] = &[
    ("Common", Color32::from_rgb(120, 120, 140)),
    ("Uncommon", Color32::from_rgb(158, 206, 106)),
    ("Rare", Color32::from_rgb(122, 162, 247)),
    ("Epic", Color32::from_rgb(187, 154, 247)),
    ("Legendary", Color32::from_rgb(224, 175, 104)),
];

fn rarity_color(rarity: usize) -> Color32 {
    RARITIES.get(rarity).map_or(RARITIES[0].1, |r| r.1)
}

fn rarity_glow(rarity: usize) -> Option<Color32> {
    if rarity >= 2 {
        let c = rarity_color(rarity);
        Some(Color32::from_rgba_premultiplied(
            c.r() / 3,
            c.g() / 3,
            c.b() / 3,
            60,
        ))
    } else {
        None
    }
}

// ============================================================================
// Texture loading
// ============================================================================

fn try_load_texture(ctx: &egui::Context, url: &str) -> Option<egui::TextureId> {
    ctx.try_load_texture(
        url,
        egui::TextureOptions::LINEAR,
        egui::load::SizeHint::default(),
    )
    .ok()
    .and_then(|poll| match poll {
        egui::load::TexturePoll::Ready { texture } => Some(texture.id),
        _ => None,
    })
}

// ============================================================================
// 3D Perspective Projection
// ============================================================================

/// Project a 2D point through 3D rotation + perspective division.
///
/// 1. Translate so `center` is the origin
/// 2. Rotate around X axis by `angle_x` (tilt forward/back)
/// 3. Rotate around Y axis by `angle_y` (tilt left/right)
/// 4. Perspective divide: x' = x * d/(d+z), y' = y * d/(d+z)
/// 5. Translate back to screen space
fn project_3d(point: Pos2, center: Pos2, angle_x: f32, angle_y: f32, perspective: f32) -> Pos2 {
    let x = point.x - center.x;
    let y = point.y - center.y;
    let z: f32 = 0.0;

    // Rotate around X axis (pitch): y' = y*cos - z*sin, z' = y*sin + z*cos
    let (sx, cx) = angle_x.sin_cos();
    let y1 = y * cx - z * sx;
    let z1 = y * sx + z * cx;

    // Rotate around Y axis (yaw): x' = x*cos + z*sin, z' = -x*sin + z*cos
    let (sy, cy) = angle_y.sin_cos();
    let x2 = x * cy + z1 * sy;
    let z2 = -x * sy + z1 * cy;

    // Perspective divide
    let scale = perspective / (perspective + z2);
    Pos2::new(center.x + x2 * scale, center.y + y1 * scale)
}

/// Project a slice of points through 3D perspective.
fn project_points(
    points: &[Pos2],
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) -> Vec<Pos2> {
    points
        .iter()
        .map(|&p| project_3d(p, center, angle_x, angle_y, perspective))
        .collect()
}

// ============================================================================
// Geometry generators
// ============================================================================

/// Vertices for a regular polygon centered at `center` with given `radius`.
fn regular_polygon_vertices(center: Pos2, radius: f32, sides: u32, rotation: f32) -> Vec<Pos2> {
    (0..sides)
        .map(|i| {
            let angle = rotation + (i as f32 / sides as f32) * TAU;
            Pos2::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

/// Vertices tracing a rounded rectangle path.
fn rounded_rect_vertices(
    center: Pos2,
    half_w: f32,
    half_h: f32,
    radius: f32,
    segments_per_corner: u32,
) -> Vec<Pos2> {
    let r = radius.min(half_w).min(half_h);
    let mut verts = Vec::new();

    let quarter = TAU / 4.0;
    // Corner centers and arc start angles (clockwise: TR → BR → BL → TL)
    let corners = [
        (center.x + half_w - r, center.y - half_h + r, -quarter), // TR
        (center.x + half_w - r, center.y + half_h - r, 0.0),      // BR
        (center.x - half_w + r, center.y + half_h - r, quarter),  // BL
        (center.x - half_w + r, center.y - half_h + r, 2.0 * quarter), // TL
    ];

    for &(cx, cy, start_angle) in &corners {
        for i in 0..=segments_per_corner {
            let t = i as f32 / segments_per_corner as f32;
            let angle = start_angle + t * quarter;
            verts.push(Pos2::new(cx + angle.cos() * r, cy + angle.sin() * r));
        }
    }

    verts
}

// ============================================================================
// Outline helpers (unified border path + spark traversal)
// ============================================================================

/// Expand a closed outline outward by `thickness` using miter normals.
/// Returns a parallel outer path with the same vertex count.
fn expand_outline(inner: &[Pos2], thickness: f32) -> Vec<Pos2> {
    let n = inner.len();
    if n < 3 {
        return inner.to_vec();
    }
    let mut outer = Vec::with_capacity(n);
    for i in 0..n {
        let prev = inner[(i + n - 1) % n];
        let curr = inner[i];
        let next = inner[(i + 1) % n];

        // Edge vectors
        let e1x = curr.x - prev.x;
        let e1y = curr.y - prev.y;
        let e2x = next.x - curr.x;
        let e2y = next.y - curr.y;

        // Outward normals (rotate edge 90° CW for clockwise winding)
        let len1 = (e1x * e1x + e1y * e1y).sqrt().max(0.001);
        let n1x = e1y / len1;
        let n1y = -e1x / len1;
        let len2 = (e2x * e2x + e2y * e2y).sqrt().max(0.001);
        let n2x = e2y / len2;
        let n2y = -e2x / len2;

        // Miter direction (average of the two normals)
        let mx = n1x + n2x;
        let my = n1y + n2y;
        let mlen = (mx * mx + my * my).sqrt().max(0.001);
        let mx = mx / mlen;
        let my = my / mlen;

        // Miter length: thickness / cos(half-angle)
        let cos_half = (mx * n1x + my * n1y).max(0.2); // clamp to avoid spikes
        let miter_len = thickness / cos_half;

        outer.push(Pos2::new(curr.x + mx * miter_len, curr.y + my * miter_len));
    }
    outer
}

/// Cumulative arc-lengths for a closed polygon path.
/// Returns a vec of length `path.len() + 1` where [0] = 0 and [n] = total perimeter.
fn cumulative_lengths(path: &[Pos2]) -> Vec<f32> {
    let n = path.len();
    let mut cum = Vec::with_capacity(n + 1);
    cum.push(0.0);
    for i in 0..n {
        let j = (i + 1) % n;
        let dx = path[j].x - path[i].x;
        let dy = path[j].y - path[i].y;
        cum.push(cum[i] + (dx * dx + dy * dy).sqrt());
    }
    cum
}

/// Sample a position along a closed polygon path at parameter `t` (0..1).
fn sample_path(path: &[Pos2], cum: &[f32], t: f32) -> Pos2 {
    let total = *cum.last().unwrap_or(&1.0);
    let target = (t.fract() + 1.0).fract() * total; // handle negative t
                                                    // Binary search for the segment
    let seg = match cum.binary_search_by(|v| v.partial_cmp(&target).unwrap()) {
        Ok(i) => i.min(path.len() - 1),
        Err(i) => (i - 1).min(path.len() - 1),
    };
    let seg_start = cum[seg];
    let seg_end = cum[seg + 1];
    let seg_len = seg_end - seg_start;
    let frac = if seg_len > 0.001 {
        (target - seg_start) / seg_len
    } else {
        0.0
    };
    let a = path[seg];
    let b = path[(seg + 1) % path.len()];
    Pos2::new(a.x + (b.x - a.x) * frac, a.y + (b.y - a.y) * frac)
}

/// Badge geometry constants.
const BADGE_H: f32 = 22.0;
const BADGE_W_FRAC: f32 = 0.45; // fraction of card width
const BADGE_OVERLAP: f32 = 0.4; // 40% overlaps card bottom
const BADGE_ARC_SEGS: u32 = 8;

/// Generate a pill (stadium) polygon as a Vec of [f32; 2] points.
fn pill_polygon(cx: f32, cy: f32, width: f32, height: f32, segs: u32) -> Vec<[f32; 2]> {
    let r = height / 2.0;
    let half_straight = (width / 2.0 - r).max(0.0);
    let mut pts = Vec::new();

    // Right cap (semicircle from -π/2 to +π/2)
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let a = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push([cx + half_straight + r * a.cos(), cy + r * a.sin()]);
    }

    // Left cap (semicircle from +π/2 to +3π/2)
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let a = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        pts.push([cx - half_straight + r * a.cos(), cy + r * a.sin()]);
    }

    pts
}

/// Build a unified outline path (clockwise) that merges the card border and
/// badge popout into a single continuous silhouette using boolean union.
///
/// For hex shapes (no badge pill), returns the plain polygon outline.
fn unified_outline(center: Pos2, half: f32, mask: CardMask) -> Vec<Pos2> {
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;

    match mask {
        CardMask::Hex { radius } => regular_polygon_vertices(center, radius, 6, -TAU / 4.0),
        CardMask::Square => {
            let left = center.x - half;
            let right = center.x + half;
            let top = center.y - half;
            let bottom = center.y + half;

            let card: Vec<[f32; 2]> =
                vec![[left, top], [right, top], [right, bottom], [left, bottom]];

            let badge_w = half * 2.0 * BADGE_W_FRAC;
            let badge_top = bottom - BADGE_H * BADGE_OVERLAP;
            let badge_cy = badge_top + BADGE_H / 2.0;
            let pill = pill_polygon(center.x, badge_cy, badge_w, BADGE_H, BADGE_ARC_SEGS);

            let result = card.overlay(&pill, OverlayRule::Union, FillRule::EvenOdd);

            if let Some(shape) = result.first() {
                if let Some(contour) = shape.first() {
                    return contour.iter().map(|p| Pos2::new(p[0], p[1])).collect();
                }
            }

            // Fallback: plain card rect
            vec![
                Pos2::new(left, top),
                Pos2::new(right, top),
                Pos2::new(right, bottom),
                Pos2::new(left, bottom),
            ]
        }
        CardMask::RoundedSquare { corner_radius } => {
            let r = corner_radius.min(half);
            let left = center.x - half;
            let right = center.x + half;
            let top = center.y - half;
            let bottom = center.y + half;

            // Build rounded rect as polygon
            let segs = 8_u32;
            let quarter = TAU / 4.0;
            let mut card: Vec<[f32; 2]> = Vec::new();

            // Top-right corner arc
            let cx_tr = right - r;
            let cy_tr = top + r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = -quarter + t * quarter;
                card.push([cx_tr + a.cos() * r, cy_tr + a.sin() * r]);
            }

            // Bottom-right corner arc
            let cx_br = right - r;
            let cy_br = bottom - r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = t * quarter;
                card.push([cx_br + a.cos() * r, cy_br + a.sin() * r]);
            }

            // Bottom-left corner arc
            let cx_bl = left + r;
            let cy_bl = bottom - r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = quarter + t * quarter;
                card.push([cx_bl + a.cos() * r, cy_bl + a.sin() * r]);
            }

            // Top-left corner arc
            let cx_tl = left + r;
            let cy_tl = top + r;
            for i in 0..=segs {
                let t = i as f32 / segs as f32;
                let a = 2.0 * quarter + t * quarter;
                card.push([cx_tl + a.cos() * r, cy_tl + a.sin() * r]);
            }

            let badge_w = half * 2.0 * BADGE_W_FRAC;
            let badge_top = bottom - BADGE_H * BADGE_OVERLAP;
            let badge_cy = badge_top + BADGE_H / 2.0;
            let pill = pill_polygon(center.x, badge_cy, badge_w, BADGE_H, BADGE_ARC_SEGS);

            let result = card.overlay(&pill, OverlayRule::Union, FillRule::EvenOdd);

            if let Some(shape) = result.first() {
                if let Some(contour) = shape.first() {
                    return contour.iter().map(|p| Pos2::new(p[0], p[1])).collect();
                }
            }

            // Fallback: plain rounded rect
            card.iter().map(|p| Pos2::new(p[0], p[1])).collect()
        }
    }
}

// ============================================================================
// Mesh drawing primitives
// ============================================================================

fn draw_quad(painter: &egui::Painter, corners: [Pos2; 4], color: Color32) {
    let mut mesh = Mesh::default();
    for &pos in &corners {
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

fn draw_textured_quad(
    painter: &egui::Painter,
    corners: [Pos2; 4],
    texture_id: egui::TextureId,
    tint: Color32,
) {
    let uvs = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    let mut mesh = Mesh::with_texture(texture_id);
    for (i, &pos) in corners.iter().enumerate() {
        mesh.vertices.push(Vertex {
            pos,
            uv: uvs[i],
            color: tint,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// Solid-colour triangle fan from center through edge vertices.
fn draw_colored_fan(painter: &egui::Painter, center: Pos2, vertices: &[Pos2], color: Color32) {
    let n = vertices.len();
    if n < 2 {
        return;
    }
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: center,
        uv: WHITE_UV,
        color,
    });
    for &v in vertices {
        mesh.vertices.push(Vertex {
            pos: v,
            uv: WHITE_UV,
            color,
        });
    }
    for i in 0..n {
        let a = (i + 1) as u32;
        let b = ((i + 1) % n + 1) as u32;
        mesh.indices.extend_from_slice(&[0, a, b]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Coloured ring (annular mesh) between inner and outer edge vertices.
fn draw_colored_ring(painter: &egui::Painter, inner: &[Pos2], outer: &[Pos2], color: Color32) {
    let n = inner.len();
    if n < 2 || outer.len() != n {
        return;
    }
    let mut mesh = Mesh::default();
    for i in 0..n {
        mesh.vertices.push(Vertex {
            pos: inner[i],
            uv: WHITE_UV,
            color,
        });
        mesh.vertices.push(Vertex {
            pos: outer[i],
            uv: WHITE_UV,
            color,
        });
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let i_in = (i * 2) as u32;
        let i_out = (i * 2 + 1) as u32;
        let j_in = (j * 2) as u32;
        let j_out = (j * 2 + 1) as u32;
        mesh.indices
            .extend_from_slice(&[i_in, i_out, j_out, i_in, j_out, j_in]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Animated spark streak that travels along a projected border path.
/// Draws a short glowing trail at `phase` (0..1) around the path.
fn draw_spark_streak(painter: &egui::Painter, path: &[Pos2], phase: f32, color: Color32) {
    if path.len() < 3 {
        return;
    }
    let cum = cumulative_lengths(path);
    let total = *cum.last().unwrap();
    if total < 1.0 {
        return;
    }

    let streak_frac = 0.08; // 8% of path length
    let samples = 20;

    let [r, g, b, _] = color.to_array();
    // Brighten toward white for the core
    let core_r = r.saturating_add((255 - r) / 2);
    let core_g = g.saturating_add((255 - g) / 2);
    let core_b = b.saturating_add((255 - b) / 2);

    let mut mesh = Mesh::default();

    for i in 0..samples {
        let frac = i as f32 / (samples - 1) as f32; // 0 = tail, 1 = head
        let t = (phase - streak_frac * (1.0 - frac)).rem_euclid(1.0);
        let pos = sample_path(path, &cum, t);

        // Cubic falloff from head to tail
        let alpha = frac * frac * frac;

        // Glow circle (larger, dimmer)
        let glow_radius = 5.0;
        let glow_alpha = (alpha * 0.3 * 255.0) as u8;
        let glow_color = Color32::from_rgba_premultiplied(
            (r as f32 * alpha * 0.3) as u8,
            (g as f32 * alpha * 0.3) as u8,
            (b as f32 * alpha * 0.3) as u8,
            glow_alpha,
        );
        let glow_segs = 6;
        let center_idx = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: glow_color,
        });
        for s in 0..glow_segs {
            let a = std::f32::consts::TAU * s as f32 / glow_segs as f32;
            mesh.vertices.push(Vertex {
                pos: Pos2::new(pos.x + a.cos() * glow_radius, pos.y + a.sin() * glow_radius),
                uv: WHITE_UV,
                color: Color32::TRANSPARENT,
            });
        }
        for s in 0..glow_segs {
            let next = (s + 1) % glow_segs;
            mesh.indices.extend_from_slice(&[
                center_idx,
                center_idx + 1 + s as u32,
                center_idx + 1 + next as u32,
            ]);
        }

        // Core circle (smaller, brighter)
        let core_radius = 2.0;
        let core_alpha = (alpha * 255.0) as u8;
        let core_color = Color32::from_rgba_premultiplied(
            (core_r as f32 * alpha) as u8,
            (core_g as f32 * alpha) as u8,
            (core_b as f32 * alpha) as u8,
            core_alpha,
        );
        let center_idx2 = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: core_color,
        });
        for s in 0..glow_segs {
            let a = std::f32::consts::TAU * s as f32 / glow_segs as f32;
            mesh.vertices.push(Vertex {
                pos: Pos2::new(pos.x + a.cos() * core_radius, pos.y + a.sin() * core_radius),
                uv: WHITE_UV,
                color: Color32::TRANSPARENT,
            });
        }
        for s in 0..glow_segs {
            let next = (s + 1) % glow_segs;
            mesh.indices.extend_from_slice(&[
                center_idx2,
                center_idx2 + 1 + s as u32,
                center_idx2 + 1 + next as u32,
            ]);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

/// Textured triangle fan. Center vertex gets UV(0.5, 0.5).
/// Edge vertex UVs computed from angle relative to center.
fn draw_textured_fan(
    painter: &egui::Painter,
    center: Pos2,
    vertices: &[Pos2],
    texture_id: egui::TextureId,
    tint: Color32,
    uv_vertices: &[Pos2],
    uv_center: Pos2,
) {
    let n = vertices.len();
    if n < 2 {
        return;
    }
    let mut mesh = Mesh::with_texture(texture_id);

    // Center UV from angle (center is always 0.5, 0.5)
    mesh.vertices.push(Vertex {
        pos: center,
        uv: Pos2::new(0.5, 0.5),
        color: tint,
    });

    // Edge vertex UVs from untilted angle relative to center
    for (i, &v) in vertices.iter().enumerate() {
        let uv_pos = uv_vertices.get(i).copied().unwrap_or(v);
        let dx = uv_pos.x - uv_center.x;
        let dy = uv_pos.y - uv_center.y;
        let angle = dy.atan2(dx);
        let uv_x = 0.5 + 0.5 * angle.cos();
        let uv_y = 0.5 + 0.5 * angle.sin();
        mesh.vertices.push(Vertex {
            pos: v,
            uv: Pos2::new(uv_x, uv_y),
            color: tint,
        });
    }

    for i in 0..n {
        let a = (i + 1) as u32;
        let b = ((i + 1) % n + 1) as u32;
        mesh.indices.extend_from_slice(&[0, a, b]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Textured triangle fan with rectangular UV mapping (for rounded rects).
/// UVs computed from `uv_vertices` against `bbox`, positions from `vertices`.
#[allow(clippy::too_many_arguments)]
fn draw_textured_fan_rect_uv(
    painter: &egui::Painter,
    center: Pos2,
    vertices: &[Pos2],
    bbox: Rect,
    texture_id: egui::TextureId,
    tint: Color32,
    uv_vertices: &[Pos2],
    uv_center: Pos2,
) {
    let n = vertices.len();
    if n < 2 {
        return;
    }
    let mut mesh = Mesh::with_texture(texture_id);

    let to_uv = |p: Pos2| {
        Pos2::new(
            ((p.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0),
            ((p.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0),
        )
    };

    mesh.vertices.push(Vertex {
        pos: center,
        uv: to_uv(uv_center),
        color: tint,
    });

    for (i, &v) in vertices.iter().enumerate() {
        let uv_pos = uv_vertices.get(i).copied().unwrap_or(v);
        mesh.vertices.push(Vertex {
            pos: v,
            uv: to_uv(uv_pos),
            color: tint,
        });
    }

    for i in 0..n {
        let a = (i + 1) as u32;
        let b = ((i + 1) % n + 1) as u32;
        mesh.indices.extend_from_slice(&[0, a, b]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

// ============================================================================
// Holographic overlay
// ============================================================================

/// Effect overlay for quad shapes (square), projected through 3D.
/// Subdivides the quad into a grid and colours vertices via the given effect.
#[allow(clippy::too_many_arguments)]
fn draw_effect_quad(
    painter: &egui::Painter,
    bbox: Rect,
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    mouse_u: f32,
    mouse_v: f32,
    effect: &dyn CardEffect,
) {
    let cols = 12_u32;
    let rows = 12_u32;

    // Build vertex positions and normalized UVs
    let mut positions = Vec::with_capacity(((rows + 1) * (cols + 1)) as usize);
    let mut effect_verts = Vec::with_capacity(positions.capacity());

    for row in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = row as f32 / rows as f32;
            let flat_pos = Pos2::new(
                bbox.left() + u * bbox.width(),
                bbox.top() + v * bbox.height(),
            );
            positions.push(project_3d(flat_pos, center, angle_x, angle_y, perspective));
            effect_verts.push(EffectVertex {
                norm_u: u,
                norm_v: v,
            });
        }
    }

    let colors = effect.compute_colors(&effect_verts, mouse_u, mouse_v);

    let mut mesh = Mesh::default();
    for (i, pos) in positions.iter().enumerate() {
        mesh.vertices.push(Vertex {
            pos: *pos,
            uv: WHITE_UV,
            color: colors[i],
        });
    }

    let stride = cols + 1;
    for row in 0..rows {
        for col in 0..cols {
            let tl = row * stride + col;
            let tr = tl + 1;
            let bl = tl + stride;
            let br = bl + 1;
            mesh.indices.extend_from_slice(&[tl, tr, bl, bl, tr, br]);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

/// Effect overlay for fan-based shapes (hex, rounded square).
/// Subdivides each triangle of the fan and colours vertices via the given effect.
fn draw_effect_fan(
    painter: &egui::Painter,
    center: Pos2,
    vertices: &[Pos2],
    bbox: Rect,
    mouse_u: f32,
    mouse_v: f32,
    effect: &dyn CardEffect,
) {
    let n = vertices.len();
    if n < 2 {
        return;
    }

    let subdivisions = 4_u32;

    let to_uv = |p: Pos2| -> (f32, f32) {
        (
            ((p.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0),
            ((p.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0),
        )
    };

    // Collect all positions and UVs first
    let mut positions = Vec::new();
    let mut effect_verts = Vec::new();
    let mut tri_index_ranges = Vec::new();

    for i in 0..n {
        let j = (i + 1) % n;
        let v0 = center;
        let v1 = vertices[i];
        let v2 = vertices[j];

        let base_idx = positions.len();
        for row in 0..=subdivisions {
            for col in 0..=(subdivisions - row) {
                let a = row as f32 / subdivisions as f32;
                let b = col as f32 / subdivisions as f32;
                let c = 1.0 - a - b;
                let pos = Pos2::new(
                    c * v0.x + a * v1.x + b * v2.x,
                    c * v0.y + a * v1.y + b * v2.y,
                );
                let (u, v) = to_uv(pos);
                positions.push(pos);
                effect_verts.push(EffectVertex {
                    norm_u: u,
                    norm_v: v,
                });
            }
        }
        tri_index_ranges.push(base_idx);
    }

    let colors = effect.compute_colors(&effect_verts, mouse_u, mouse_v);

    let mut mesh = Mesh::default();
    for (i, pos) in positions.iter().enumerate() {
        mesh.vertices.push(Vertex {
            pos: *pos,
            uv: WHITE_UV,
            color: colors[i],
        });
    }

    // Index the subdivided triangles
    for (seg, &base_idx) in tri_index_ranges.iter().enumerate() {
        let _ = seg;
        let base = base_idx as u32;
        let mut row_start = base;
        for row in 0..subdivisions {
            let row_len = subdivisions - row + 1;
            let next_row_start = row_start + row_len;
            for col in 0..(row_len - 1) {
                let tl = row_start + col;
                let tr = row_start + col + 1;
                let bl = next_row_start + col;
                mesh.indices.extend_from_slice(&[tl, tr, bl]);
                if col < row_len - 2 {
                    let br = next_row_start + col + 1;
                    mesh.indices.extend_from_slice(&[tr, br, bl]);
                }
            }
            row_start = next_row_start;
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

// ============================================================================
// Tilted text
// ============================================================================

/// Project a pre-laid-out galley through 3D perspective, optionally with text shadows.
#[allow(clippy::too_many_arguments)]
fn draw_projected_galley(
    ui: &egui::Ui,
    painter: &egui::Painter,
    galley: &std::sync::Arc<egui::Galley>,
    text_pos: Pos2,
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    shadow: bool,
) {
    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

    // Shadow pass
    if shadow {
        let shadow_color = Color32::from_rgba_premultiplied(0, 0, 0, 180);
        for &(dx, dy) in &[(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let mut shadow_mesh = Mesh::with_texture(egui::TextureId::default());
            for placed_row in &galley.rows {
                let row_offset = placed_row.pos;
                let row_mesh = &placed_row.row.visuals.mesh;
                let idx_offset = shadow_mesh.vertices.len() as u32;
                for vertex in &row_mesh.vertices {
                    let abs = Pos2::new(
                        text_pos.x + row_offset.x + vertex.pos.x + dx,
                        text_pos.y + row_offset.y + vertex.pos.y + dy,
                    );
                    let projected = project_3d(abs, center, angle_x, angle_y, perspective);
                    let norm_uv = Pos2::new(vertex.uv.x * uv_norm.x, vertex.uv.y * uv_norm.y);
                    shadow_mesh.vertices.push(Vertex {
                        pos: projected,
                        uv: norm_uv,
                        color: shadow_color,
                    });
                }
                for &idx in &row_mesh.indices {
                    shadow_mesh.indices.push(idx + idx_offset);
                }
            }
            painter.add(egui::Shape::mesh(shadow_mesh));
        }
    } // end if shadow

    // Foreground
    let mut text_mesh = Mesh::with_texture(egui::TextureId::default());
    for placed_row in &galley.rows {
        let row_offset = placed_row.pos;
        let row_mesh = &placed_row.row.visuals.mesh;
        let idx_offset = text_mesh.vertices.len() as u32;
        for vertex in &row_mesh.vertices {
            let abs = Pos2::new(
                text_pos.x + row_offset.x + vertex.pos.x,
                text_pos.y + row_offset.y + vertex.pos.y,
            );
            let projected = project_3d(abs, center, angle_x, angle_y, perspective);
            let norm_uv = Pos2::new(vertex.uv.x * uv_norm.x, vertex.uv.y * uv_norm.y);
            text_mesh.vertices.push(Vertex {
                pos: projected,
                uv: norm_uv,
                color: vertex.color,
            });
        }
        for &idx in &row_mesh.indices {
            text_mesh.indices.push(idx + idx_offset);
        }
    }
    painter.add(egui::Shape::mesh(text_mesh));
}

#[allow(clippy::too_many_arguments)]
fn draw_projected_text(
    ui: &egui::Ui,
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    text_pos: Pos2,
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) {
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    draw_projected_galley(
        ui,
        painter,
        &galley,
        text_pos,
        center,
        angle_x,
        angle_y,
        perspective,
        true,
    );
}

/// Stat definitions for the right-edge vertical stack.
struct StatEntry {
    icon: PhosphorIcon,
    value: &'static str,
    color: Color32,
}

const DEMO_STATS: &[StatEntry] = &[
    StatEntry {
        icon: PhosphorIcon::Sword,
        value: "5",
        color: Color32::from_rgb(255, 130, 100),
    },
    StatEntry {
        icon: PhosphorIcon::Shield,
        value: "3",
        color: Color32::from_rgb(100, 160, 255),
    },
    StatEntry {
        icon: PhosphorIcon::Lightning,
        value: "7",
        color: Color32::from_rgb(255, 220, 50),
    },
    StatEntry {
        icon: PhosphorIcon::Heart,
        value: "12",
        color: Color32::from_rgb(255, 80, 120),
    },
];

// ============================================================================
// Shape-aware overlay positioning
// ============================================================================

/// Card mask type — controls how overlay elements position relative to the shape.
#[derive(Clone, Copy)]
enum CardMask {
    Square,
    Hex { radius: f32 },
    RoundedSquare { corner_radius: f32 },
}

impl CardMask {
    /// Right edge X at a given Y, relative to shape center.
    /// Returns the X coordinate of the shape's right boundary at `y`.
    fn right_edge_at(&self, center: Pos2, half: f32, y: f32) -> f32 {
        match self {
            CardMask::Square => center.x + half,
            CardMask::Hex { radius } => {
                // Pointy-top hex vertices (rotation = -90°):
                //   V0 top (cx, cy-r), V1 upper-right (cx+r√3/2, cy-r/2),
                //   V2 lower-right (cx+r√3/2, cy+r/2), V3 bottom (cx, cy+r)
                // Right edge has 3 segments:
                //   NE edge (V0→V1): slopes from (cx, cy-r) to (cx+r√3/2, cy-r/2)
                //   E edge  (V1→V2): vertical at x = cx+r√3/2, from cy-r/2 to cy+r/2
                //   SE edge (V2→V3): slopes from (cx+r√3/2, cy+r/2) to (cx, cy+r)
                let max_half_w = radius * (std::f32::consts::PI / 6.0).cos(); // r√3/2
                let dy = y - center.y;
                let half_r = radius * 0.5;
                if dy.abs() <= half_r {
                    // Vertical right edge (between upper-right and lower-right vertices)
                    center.x + max_half_w
                } else {
                    // Sloping edge (NE or SE): linear from max width at ±r/2 to 0 at ±r
                    let overshoot = dy.abs() - half_r;
                    let slope_len = radius - half_r; // = r/2
                    let frac = (overshoot / slope_len).clamp(0.0, 1.0);
                    center.x + max_half_w * (1.0 - frac)
                }
            }
            CardMask::RoundedSquare { corner_radius } => {
                let r = *corner_radius;
                let dy = (y - center.y).abs();
                let straight_half = half - r;
                if dy <= straight_half {
                    // In the straight section
                    center.x + half
                } else {
                    // In the corner arc
                    let arc_dy = dy - straight_half;
                    let arc_dx = (r * r - arc_dy * arc_dy).max(0.0).sqrt();
                    center.x + straight_half + arc_dx
                }
            }
        }
    }

    /// Title Y position (inside the shape's top area).
    fn title_y(&self, center: Pos2, half: f32) -> f32 {
        match self {
            CardMask::Square => center.y - half + half * 0.14,
            CardMask::Hex { radius } => center.y - radius * 0.72,
            CardMask::RoundedSquare { .. } => center.y - half + half * 0.14,
        }
    }

    /// Bottom anchor Y for badge popout.
    fn badge_anchor_y(&self, center: Pos2, half: f32) -> f32 {
        match self {
            CardMask::Square => center.y + half,
            CardMask::Hex { radius } => center.y + *radius,
            CardMask::RoundedSquare { .. } => center.y + half,
        }
    }
}

/// Draw a projected rounded-left / hard-right pill background for a stat.
/// Split into a half-circle fan (left cap) + a rectangle quad (body).
#[allow(clippy::too_many_arguments)]
fn draw_stat_pill(
    painter: &egui::Painter,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    color: Color32,
) {
    let r = height / 2.0;
    let cap_cx = left + r;
    let cap_cy = top + r;
    let right = left + width;

    // 1. Left half-circle cap (fan from arc center)
    let arc_segs = 10;
    let mut arc_verts = Vec::new();
    for i in 0..=arc_segs {
        let t = i as f32 / arc_segs as f32;
        let angle = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        arc_verts.push(Pos2::new(
            cap_cx + angle.cos() * r,
            cap_cy - angle.sin() * r,
        ));
    }
    let proj_arc = project_points(&arc_verts, center, angle_x, angle_y, perspective);
    let proj_cap_center = project_3d(
        Pos2::new(cap_cx, cap_cy),
        center,
        angle_x,
        angle_y,
        perspective,
    );
    draw_colored_fan(painter, proj_cap_center, &proj_arc, color);

    // 2. Rectangular body (from cap edge to hard right)
    let body = [
        Pos2::new(cap_cx, top),
        Pos2::new(right, top),
        Pos2::new(right, top + height),
        Pos2::new(cap_cx, top + height),
    ];
    let proj_body = project_points(&body, center, angle_x, angle_y, perspective);
    draw_quad(
        painter,
        [proj_body[0], proj_body[1], proj_body[2], proj_body[3]],
        color,
    );
}

/// Draw stat pills stacked vertically, anchored to shape's right edge.
#[allow(clippy::too_many_arguments)]
fn draw_stat_stack(
    ui: &egui::Ui,
    painter: &egui::Painter,
    center: Pos2,
    half: f32,
    mask: CardMask,
    proj_center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    pill_bg: Color32,
) {
    let stat_font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let icon_font = egui::FontId::new(12.0, phosphor_family());
    let pill_w = 46.0;
    let pill_h = 16.0;
    let pill_gap = 3.0;
    let total_h =
        DEMO_STATS.len() as f32 * pill_h + (DEMO_STATS.len() - 1).max(0) as f32 * pill_gap;
    let stack_top = center.y - total_h / 2.0;

    for (i, stat) in DEMO_STATS.iter().enumerate() {
        let y = stack_top + i as f32 * (pill_h + pill_gap);
        let pill_center_y = y + pill_h / 2.0;
        let pill_right = mask.right_edge_at(center, half, pill_center_y);
        let pill_left = pill_right - pill_w;

        draw_stat_pill(
            painter,
            pill_left,
            y,
            pill_w,
            pill_h,
            proj_center,
            angle_x,
            angle_y,
            perspective,
            pill_bg,
        );

        let mut job = LayoutJob::default();
        job.append(
            &stat.icon.as_str(),
            0.0,
            TextFormat::simple(icon_font.clone(), stat.color),
        );
        job.append(
            &format!(" {}", stat.value),
            0.0,
            TextFormat::simple(stat_font.clone(), stat.color),
        );
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        let text_x = pill_left + (pill_w - galley.size().x) / 2.0;
        let text_y = y + (pill_h - galley.size().y) / 2.0;
        draw_projected_galley(
            ui,
            painter,
            &galley,
            Pos2::new(text_x, text_y),
            proj_center,
            angle_x,
            angle_y,
            perspective,
            false,
        );
    }
}

/// Hex-specific overlay: stats pinned to NE edge, name at bottom, badge text below.
#[allow(clippy::too_many_arguments)]
fn draw_hex_overlay(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rarity: usize,
    center: Pos2,
    half: f32,
    mask: CardMask,
    proj_center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    text_color: Color32,
    rarity_col: Color32,
    pill_bg: Color32,
) {
    let radius = match mask {
        CardMask::Hex { radius } => radius,
        _ => half,
    };

    // --- Stats on right edge (vertical edge of pointy-top hex) ---
    // The right edge (V1→V2) is perfectly vertical, so use the standard
    // horizontal pill stack, same as square/rounded-square.
    draw_stat_stack(
        ui,
        painter,
        center,
        half,
        mask,
        proj_center,
        angle_x,
        angle_y,
        perspective,
        pill_bg,
    );

    // --- Name at bottom of hex ---
    let title_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let name_y = center.y + radius * 0.62;
    let title_g =
        painter.layout_no_wrap("Shadow Drake".to_string(), title_font.clone(), text_color);
    let title_pos = Pos2::new(
        center.x - title_g.size().x / 2.0,
        name_y - title_g.size().y / 2.0,
    );
    draw_projected_text(
        ui,
        painter,
        "Shadow Drake",
        title_font,
        text_color,
        title_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
    );

    // --- Badge text below name (no background, just rarity stars + type) ---
    let badge_font = egui::FontId::new(10.0, phosphor_family());
    let badge_text_font = egui::FontId::new(9.0, egui::FontFamily::Monospace);
    let badge_y = name_y + 14.0;

    let mut badge_job = LayoutJob::default();
    badge_job.append(
        &PhosphorIcon::Star.as_str(),
        0.0,
        TextFormat::simple(badge_font.clone(), rarity_col),
    );
    badge_job.append(
        " Dragon ",
        0.0,
        TextFormat::simple(badge_text_font, text_color),
    );
    badge_job.append(
        &PhosphorIcon::Star.as_str(),
        0.0,
        TextFormat::simple(badge_font, rarity_col),
    );
    let badge_g = ui.ctx().fonts_mut(|f| f.layout_job(badge_job));
    let badge_pos = Pos2::new(
        center.x - badge_g.size().x / 2.0,
        badge_y - badge_g.size().y / 2.0,
    );
    draw_projected_galley(
        ui,
        painter,
        &badge_g,
        badge_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
        true,
    );

    let _ = rarity;
}

/// Draw title, stat pills, and badge — layout varies by mask shape.
#[allow(clippy::too_many_arguments)]
fn draw_tile_overlay(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rarity: usize,
    bbox: Rect,
    mask: CardMask,
    proj_center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let (_rarity_name, rarity_col) = RARITIES.get(rarity).copied().unwrap_or(RARITIES[0]);
    let pill_bg = Color32::from_rgba_premultiplied(20, 20, 35, 200);
    let center = bbox.center();
    let half = bbox.width() / 2.0;

    if matches!(mask, CardMask::Hex { .. }) {
        // === Hex-specific layout ===
        // Stats pinned to NE edge, name at bottom, badge text below name (no bg)
        draw_hex_overlay(
            ui,
            painter,
            rarity,
            center,
            half,
            mask,
            proj_center,
            angle_x,
            angle_y,
            perspective,
            text_color,
            rarity_col,
            pill_bg,
        );
        return;
    }

    // === Square / RoundedSquare layout ===

    // --- Title (top center, inside the shape) ---
    let title_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let title_y = mask.title_y(center, half);
    let title_g =
        painter.layout_no_wrap("Shadow Drake".to_string(), title_font.clone(), text_color);
    let title_pos = Pos2::new(
        center.x - title_g.size().x / 2.0,
        title_y - title_g.size().y / 2.0,
    );
    draw_projected_text(
        ui,
        painter,
        "Shadow Drake",
        title_font,
        text_color,
        title_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
    );

    // --- Stats (pill stack, anchored to shape's right edge, vertically centered) ---
    draw_stat_stack(
        ui,
        painter,
        center,
        half,
        mask,
        proj_center,
        angle_x,
        angle_y,
        perspective,
        pill_bg,
    );

    // --- Badge popout (extends below shape's bottom edge) ---
    let badge_font = egui::FontId::new(11.0, phosphor_family());
    let badge_text_font = egui::FontId::new(9.0, egui::FontFamily::Monospace);
    let badge_h = 22.0;
    let badge_w = bbox.width() * 0.45;
    let badge_anchor = mask.badge_anchor_y(center, half);
    let badge_top = badge_anchor - badge_h * 0.4; // 40% overlap, 60% popout
    let badge_left = center.x - badge_w / 2.0;

    // Badge background: rounded rect (all corners)
    let badge_r = badge_h / 2.0;
    let badge_cx = badge_left + badge_r;
    let badge_cy = badge_top + badge_r;
    let badge_right = badge_left + badge_w;
    let badge_rcx = badge_right - badge_r;

    let mut badge_verts = Vec::new();
    let arc_segs = 6;
    // Left arc
    for i in 0..=arc_segs {
        let t = i as f32 / arc_segs as f32;
        let a = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        badge_verts.push(Pos2::new(
            badge_cx + a.cos() * badge_r,
            badge_cy - a.sin() * badge_r,
        ));
    }
    // Right arc
    for i in 0..=arc_segs {
        let t = i as f32 / arc_segs as f32;
        let a = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        badge_verts.push(Pos2::new(
            badge_rcx + a.cos() * badge_r,
            badge_cy - a.sin() * badge_r,
        ));
    }

    let badge_center_pt = Pos2::new(center.x, badge_top + badge_h / 2.0);
    let proj_badge_verts = project_points(&badge_verts, proj_center, angle_x, angle_y, perspective);
    let proj_badge_center = project_3d(badge_center_pt, proj_center, angle_x, angle_y, perspective);
    draw_colored_fan(painter, proj_badge_center, &proj_badge_verts, pill_bg);

    // Badge content: rarity stars + label
    let mut badge_job = LayoutJob::default();
    badge_job.append(
        &PhosphorIcon::Star.as_str(),
        0.0,
        TextFormat::simple(badge_font.clone(), rarity_col),
    );
    badge_job.append(
        " Dragon ",
        0.0,
        TextFormat::simple(badge_text_font, text_color),
    );
    badge_job.append(
        &PhosphorIcon::Star.as_str(),
        0.0,
        TextFormat::simple(badge_font, rarity_col),
    );
    let badge_g = ui.ctx().fonts_mut(|f| f.layout_job(badge_job));
    let badge_text_pos = Pos2::new(
        center.x - badge_g.size().x / 2.0,
        badge_top + (badge_h - badge_g.size().y) / 2.0,
    );
    draw_projected_galley(
        ui,
        painter,
        &badge_g,
        badge_text_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
        false,
    );
}

// ============================================================================
// Shared helpers
// ============================================================================

/// Update tilt state from hover, return angles in radians for 3D projection.
fn update_tilt(
    response: &egui::Response,
    center: Pos2,
    half: f32,
    tilt: &mut TiltState,
    ease: f32,
    max_angle_deg: f32,
) -> (f32, f32) {
    let (target_x, target_y) = if let Some(hover_pos) = response.hover_pos() {
        let rel_x = (hover_pos.x - center.x) / half;
        let rel_y = (hover_pos.y - center.y) / half;
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };
    tilt.current_x += (target_x - tilt.current_x) * ease;
    tilt.current_y += (target_y - tilt.current_y) * ease;

    let max_rad = max_angle_deg.to_radians();
    // angle_x = pitch (tilt forward/back from mouse Y)
    // angle_y = yaw (tilt left/right from mouse X)
    // Note: mouse Y → rotateX, mouse X → rotateY (like CSS)
    let angle_x = tilt.current_y * max_rad;
    let angle_y = -tilt.current_x * max_rad;
    (angle_x, angle_y)
}

// ============================================================================
// Individual shape demos
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn demo_square(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let half = size / 2.0;
    let padding = 40.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, half, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    // Untilted corners: TL, TR, BR, BL
    let card_rect = Rect::from_center_size(center, Vec2::splat(size));
    let corners = [
        card_rect.left_top(),
        card_rect.right_top(),
        card_rect.right_bottom(),
        card_rect.left_bottom(),
    ];
    let projected: Vec<Pos2> = project_points(&corners, center, ax, ay, perspective);
    let proj4: [Pos2; 4] = [projected[0], projected[1], projected[2], projected[3]];

    // Unified border (card + badge silhouette)
    let outline = unified_outline(center, half, CardMask::Square);
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Rarity glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill
    if let Some(tex) = art_tex {
        draw_textured_quad(&painter, proj4, tex, Color32::WHITE);
    } else {
        draw_quad(&painter, proj4, Color32::from_rgb(30, 30, 48));
    }

    // Effect overlay (Rare+, only while hovering)
    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - card_rect.left()) / card_rect.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - card_rect.top()) / card_rect.height()).clamp(0.0, 1.0);
            draw_effect_quad(
                &painter,
                card_rect,
                center,
                ax,
                ay,
                perspective,
                mu,
                mv,
                effect,
            );
        }
    }

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        card_rect,
        CardMask::Square,
        center,
        ax,
        ay,
        perspective,
    );
}

#[allow(clippy::too_many_arguments)]
fn demo_hex(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let radius = size / 2.0;
    let padding = 50.0;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, radius, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    // Unified border
    let outline = unified_outline(center, radius, CardMask::Hex { radius });
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill
    let art_center = project_3d(center, center, ax, ay, perspective);
    if let Some(tex) = art_tex {
        draw_textured_fan(
            &painter,
            art_center,
            &proj_outline,
            tex,
            Color32::WHITE,
            &outline,
            center,
        );
    } else {
        draw_colored_fan(
            &painter,
            art_center,
            &proj_outline,
            Color32::from_rgb(30, 30, 48),
        );
    }

    // Effect overlay (Rare+, only while hovering)
    let hex_bbox = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - hex_bbox.left()) / hex_bbox.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - hex_bbox.top()) / hex_bbox.height()).clamp(0.0, 1.0);
            draw_effect_fan(
                &painter,
                art_center,
                &proj_outline,
                hex_bbox,
                mu,
                mv,
                effect,
            );
        }
    }

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        hex_bbox,
        CardMask::Hex { radius },
        center,
        ax,
        ay,
        perspective,
    );
}

#[allow(clippy::too_many_arguments)]
fn demo_rounded_square(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
    effect: &dyn CardEffect,
    spark_phase: f32,
    spark_enabled: bool,
) {
    let half = size / 2.0;
    let corner_radius = size * 0.15;
    let padding = 40.0;
    let segs = 8;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(size + padding, size + padding),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();

    let (ax, ay) = update_tilt(&response, center, half, tilt, tilt_ease, max_tilt);
    if tilt.current_x.abs() > 0.001 || tilt.current_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    let bbox = Rect::from_center_size(center, Vec2::splat(size));

    // Unified border (card + badge silhouette)
    let outline = unified_outline(center, half, CardMask::RoundedSquare { corner_radius });
    let border_outer = expand_outline(&outline, 3.0);
    let proj_outline = project_points(&outline, center, ax, ay, perspective);
    let proj_border = project_points(&border_outer, center, ax, ay, perspective);

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_outer = expand_outline(&outline, 6.0);
        let proj_glow = project_points(&glow_outer, center, ax, ay, perspective);
        draw_colored_ring(&painter, &proj_border, &proj_glow, glow);
    }

    // Rarity border ring
    draw_colored_ring(&painter, &proj_outline, &proj_border, rarity_color(rarity));

    // Art fill (uses plain rounded rect, not the badge-merged outline)
    let art_verts = rounded_rect_vertices(center, half, half, corner_radius, segs);
    let art_proj = project_points(&art_verts, center, ax, ay, perspective);
    let art_center = project_3d(center, center, ax, ay, perspective);
    if let Some(tex) = art_tex {
        draw_textured_fan_rect_uv(
            &painter,
            art_center,
            &art_proj,
            bbox,
            tex,
            Color32::WHITE,
            &art_verts,
            center,
        );
    } else {
        draw_colored_fan(
            &painter,
            art_center,
            &art_proj,
            Color32::from_rgb(30, 30, 48),
        );
    }

    // Effect overlay (Rare+, only while hovering)
    if rarity >= 2 {
        if let Some(hover_pos) = response.hover_pos() {
            let mu = ((hover_pos.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0);
            let mv = ((hover_pos.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0);
            draw_effect_fan(&painter, art_center, &art_proj, bbox, mu, mv, effect);
        }
    }

    // Spark traversal
    if spark_enabled && rarity >= 2 {
        draw_spark_streak(&painter, &proj_outline, spark_phase, rarity_color(rarity));
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(
        ui,
        &painter,
        rarity,
        bbox,
        CardMask::RoundedSquare { corner_radius },
        center,
        ax,
        ay,
        perspective,
    );
}

// ============================================================================
// Main show function
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut NftShapesState) {
    // Spark animation tick
    if state.spark_enabled && state.rarity >= 2 {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        state.spark_phase = (state.spark_phase + dt * state.spark_speed) % 1.0;
        ui.ctx().request_repaint();
    }

    let art_texture = try_load_texture(ui.ctx(), IIIF_ART_URL);

    // Shared controls
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.size, 100.0..=300.0).text("Size"));
        ui.add(
            egui::Slider::new(&mut state.perspective_distance, 200.0..=2000.0).text("Perspective"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Rarity:");
        for (i, (name, color)) in RARITIES.iter().enumerate() {
            let text = if state.rarity == i {
                egui::RichText::new(*name).color(*color).strong()
            } else {
                egui::RichText::new(*name).color(TEXT_MUTED)
            };
            if ui.selectable_label(state.rarity == i, text).clicked() {
                state.rarity = i;
            }
        }
    });

    // Effect controls (only visible for Rare+)
    if state.rarity >= 2 {
        ui.horizontal(|ui| {
            ui.label("Effect:");
            egui::ComboBox::from_id_salt("effect_selector")
                .selected_text(EFFECT_NAMES[state.effect_index])
                .show_ui(ui, |ui| {
                    for (i, name) in EFFECT_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut state.effect_index, i, *name);
                    }
                });
        });

        match state.effect_index {
            0 => {
                // Streak Holo params
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.hue_range, 0.0..=180.0).text("Hue Range"));
                    ui.add(
                        egui::Slider::new(&mut state.shimmer_width, 0.05..=0.5).text("Shimmer W"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.shimmer_intensity, 0.0..=1.0)
                            .text("Intensity"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.overlay_opacity, 0.0..=0.5).text("Opacity"),
                    );
                });
            }
            1 => {
                // Thin Film params
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.iri_min, 100.0..=500.0).text("Film Min (nm)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.iri_range, 100.0..=800.0)
                            .text("Film Range (nm)"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.fresnel_power, 1.0..=10.0)
                            .text("Fresnel Power"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.iri_intensity, 0.05..=1.0).text("Intensity"),
                    );
                });
            }
            2 => {
                // Diffraction Grating params
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.grating_spacing, 800.0..=3000.0)
                            .text("Spacing"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.grating_angle, 0.0..=std::f32::consts::PI)
                            .text("Angle"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.max_orders, 1..=8).text("Orders"));
                    ui.add(
                        egui::Slider::new(&mut state.diffraction_intensity, 0.5..=3.0)
                            .text("Intensity"),
                    );
                });
            }
            3 => {
                // Glitter params
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.glitter_scale, 10.0..=80.0).text("Scale"));
                    ui.add(
                        egui::Slider::new(&mut state.sparkle_sharpness, 50.0..=500.0)
                            .text("Sharpness"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.sparkle_threshold, 0.0..=1.0)
                            .text("Threshold"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.glitter_z_depth, 0.1..=1.0).text("Z Depth"),
                    );
                });
            }
            4 => {
                // Brushed Metal params
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.roughness_along, 0.01..=0.5)
                            .text("Roughness Along"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.roughness_perp, 0.1..=2.0)
                            .text("Roughness Perp"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.brush_angle, 0.0..=std::f32::consts::PI)
                            .text("Brush Angle"),
                    );
                    ui.label("Metal:");
                    for (i, name) in ["Silver", "Gold", "Copper"].iter().enumerate() {
                        if ui
                            .selectable_label(state.metal_preset == i, *name)
                            .clicked()
                        {
                            state.metal_preset = i;
                        }
                    }
                });
            }
            5 => {
                // Aurora params
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.aurora_freq1, 3.0..=15.0).text("Freq 1"));
                    ui.add(egui::Slider::new(&mut state.aurora_freq2, 3.0..=15.0).text("Freq 2"));
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.curtain_sharpness, 2.0..=8.0)
                            .text("Sharpness"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.vertical_falloff, 0.5..=3.0).text("V Falloff"),
                    );
                    ui.add(
                        egui::Slider::new(&mut state.aurora_brightness, 0.3..=2.0)
                            .text("Brightness"),
                    );
                });
            }
            6 => {
                // Prismatic params
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut state.prism_dispersion, 0.01..=0.15)
                            .text("Dispersion"),
                    );
                    ui.add(egui::Slider::new(&mut state.prism_spread, 0.02..=0.2).text("Spread"));
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut state.facet_scale, 4.0..=20.0).text("Facets"));
                    ui.add(
                        egui::Slider::new(&mut state.prism_intensity, 0.5..=3.0).text("Intensity"),
                    );
                });
            }
            _ => {}
        }

        // Spark controls
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.spark_enabled, "Spark");
            if state.spark_enabled {
                ui.add(egui::Slider::new(&mut state.spark_speed, 0.1..=1.0).text("Speed"));
            }
        });
    }

    // Build the selected effect
    let metal_colors: [(f32, f32, f32); 3] = [
        (0.75, 0.78, 0.82), // silver
        (1.00, 0.84, 0.00), // gold
        (0.72, 0.45, 0.20), // copper
    ];
    let (mr, mg, mb) = metal_colors[state.metal_preset.min(2)];

    let effect: Box<dyn CardEffect> = match state.effect_index {
        1 => Box::new(ThinFilmIridescence {
            iri_min: state.iri_min,
            iri_range: state.iri_range,
            fresnel_power: state.fresnel_power,
            intensity: state.iri_intensity,
        }),
        2 => Box::new(DiffractionGrating {
            grating_spacing: state.grating_spacing,
            grating_angle: state.grating_angle,
            max_orders: state.max_orders,
            intensity: state.diffraction_intensity,
        }),
        3 => Box::new(Glitter {
            grid_scale: state.glitter_scale,
            sparkle_sharpness: state.sparkle_sharpness,
            sparkle_threshold: state.sparkle_threshold,
            z_depth: state.glitter_z_depth,
        }),
        4 => Box::new(BrushedMetal {
            roughness_along: state.roughness_along,
            roughness_perp: state.roughness_perp,
            brush_angle: state.brush_angle,
            metal_r: mr,
            metal_g: mg,
            metal_b: mb,
        }),
        5 => Box::new(AuroraCurtain {
            freq1: state.aurora_freq1,
            freq2: state.aurora_freq2,
            curtain_sharpness: state.curtain_sharpness,
            vertical_falloff: state.vertical_falloff,
            brightness: state.aurora_brightness,
        }),
        6 => Box::new(PrismaticDispersion {
            dispersion: state.prism_dispersion,
            spread: state.prism_spread,
            facet_scale: state.facet_scale,
            intensity: state.prism_intensity,
        }),
        _ => Box::new(StreakHolo {
            hue_range: state.hue_range,
            shimmer_width: state.shimmer_width,
            shimmer_intensity: state.shimmer_intensity,
            overlay_opacity: state.overlay_opacity,
        }),
    };

    let art_status = if art_texture.is_some() {
        "loaded"
    } else {
        "loading..."
    };
    ui.label(
        egui::RichText::new(format!("Art: {art_status}"))
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    // --- 1. Square Tile ---
    ui.label(egui::RichText::new("1. Square Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Natural 1:1 fit. Art-first with right-edge stats and holographic foil.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_square(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[0],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );
    ui.add_space(16.0);

    // --- 2. Hex Tile ---
    ui.label(egui::RichText::new("2. Hex Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Pointy-top hexagon. Tessellates for game boards. Holographic foil on Rare+.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_hex(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[1],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );
    ui.add_space(16.0);

    // --- 3. Rounded Square ---
    ui.label(
        egui::RichText::new("3. Rounded Square")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modern app-icon aesthetic. Generous corner radius with holographic foil.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);
    demo_rounded_square(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[2],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
        &*effect,
        state.spark_phase,
        state.spark_enabled,
    );

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Key observations:")
            .color(ACCENT)
            .strong(),
    );
    ui.label("- Square: best art utilisation, familiar grid layout");
    ui.label("- Hex: strongest game-board identity, tessellation-ready");
    ui.label("- Rounded Square: modern feel, good compromise");
    ui.label("- Holographic: specular streak + iridescence + fresnel glow (Rare+)");
}
