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

// ============================================================================
// State
// ============================================================================

pub struct NftShapesState {
    pub size: f32,
    pub rarity: usize,
    pub tilt_ease: f32,
    pub perspective_distance: f32,
    // Holographic overlay
    pub hue_range: f32,
    pub shimmer_width: f32,
    pub shimmer_intensity: f32,
    pub overlay_opacity: f32,
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
            hue_range: 60.0,
            shimmer_width: 0.15,
            shimmer_intensity: 0.4,
            overlay_opacity: 0.2,
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
// Colour utilities
// ============================================================================

fn hue_to_rgb(hue: f32) -> Color32 {
    let h = ((hue % 360.0) + 360.0) % 360.0 / 60.0;
    let i = h.floor() as u8;
    let f = h - h.floor();
    let q = (255.0 * (1.0 - f)) as u8;
    let t = (255.0 * f) as u8;
    match i {
        0 => Color32::from_rgb(255, t, 0),
        1 => Color32::from_rgb(q, 255, 0),
        2 => Color32::from_rgb(0, 255, t),
        3 => Color32::from_rgb(0, q, 255),
        4 => Color32::from_rgb(t, 0, 255),
        _ => Color32::from_rgb(255, 0, q),
    }
}

/// Compute holo vertex colour from normalised position and light direction.
#[allow(clippy::too_many_arguments)]
fn holo_color(
    u: f32,
    v: f32,
    mouse_u: f32,
    mouse_v: f32,
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
) -> Color32 {
    let light_x = mouse_u - 0.5;
    let light_y = mouse_v - 0.5;
    let light_len = (light_x * light_x + light_y * light_y).sqrt().max(0.001);
    let light_dx = light_x / light_len;
    let light_dy = light_y / light_len;

    let dx = u - mouse_u;
    let dy = v - mouse_v;
    let dot = dx * light_dx + dy * light_dy;
    let streak_dist = dot.abs();
    let streak = (-streak_dist * streak_dist / (shimmer_width * shimmer_width * 0.5)).exp()
        * shimmer_intensity;
    let cross = dx * light_dy - dy * light_dx;
    let hue = 200.0 + cross * hue_range * 2.0;

    let edge_u = (0.5 - (u - 0.5).abs()) * 2.0;
    let edge_v = (0.5 - (v - 0.5).abs()) * 2.0;
    let fresnel = 1.0 - edge_u.min(edge_v).clamp(0.0, 1.0);
    let fresnel_boost = fresnel * fresnel * overlay_opacity * 0.5;
    let intensity = (streak + fresnel_boost).clamp(0.0, 1.0);

    let rainbow = hue_to_rgb(hue);
    let alpha = (intensity * 255.0) as u8;
    Color32::from_rgba_premultiplied(
        (rainbow.r() as f32 * intensity) as u8,
        (rainbow.g() as f32 * intensity) as u8,
        (rainbow.b() as f32 * intensity) as u8,
        alpha,
    )
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

/// Holographic overlay for quad shapes (square), projected through 3D.
/// Subdivides the quad into a grid and colours each vertex based on mouse position.
#[allow(clippy::too_many_arguments)]
fn draw_holo_quad(
    painter: &egui::Painter,
    bbox: Rect,
    center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
    mouse_u: f32,
    mouse_v: f32,
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
) {
    let cols = 12_u32;
    let rows = 12_u32;
    let mut mesh = Mesh::default();

    for row in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = row as f32 / rows as f32;
            let flat_pos = Pos2::new(
                bbox.left() + u * bbox.width(),
                bbox.top() + v * bbox.height(),
            );
            let pos = project_3d(flat_pos, center, angle_x, angle_y, perspective);
            let color = holo_color(
                u,
                v,
                mouse_u,
                mouse_v,
                hue_range,
                shimmer_width,
                shimmer_intensity,
                overlay_opacity,
            );
            mesh.vertices.push(Vertex {
                pos,
                uv: WHITE_UV,
                color,
            });
        }
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

/// Holographic overlay for fan-based shapes (hex, rounded square).
/// Subdivides each triangle of the fan and colours vertices based on mouse position.
#[allow(clippy::too_many_arguments)]
fn draw_holo_fan(
    painter: &egui::Painter,
    center: Pos2,
    vertices: &[Pos2],
    bbox: Rect,
    mouse_u: f32,
    mouse_v: f32,
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
) {
    let n = vertices.len();
    if n < 2 {
        return;
    }

    let subdivisions = 4_u32;
    let mut mesh = Mesh::default();

    let to_uv = |p: Pos2| -> (f32, f32) {
        (
            ((p.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0),
            ((p.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0),
        )
    };

    for i in 0..n {
        let j = (i + 1) % n;
        let v0 = center;
        let v1 = vertices[i];
        let v2 = vertices[j];

        let base_idx = mesh.vertices.len() as u32;

        // Subdivide the triangle using barycentric coordinates
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
                let color = holo_color(
                    u,
                    v,
                    mouse_u,
                    mouse_v,
                    hue_range,
                    shimmer_width,
                    shimmer_intensity,
                    overlay_opacity,
                );
                mesh.vertices.push(Vertex {
                    pos,
                    uv: WHITE_UV,
                    color,
                });
            }
        }

        // Index the subdivided triangle
        let mut row_start = base_idx;
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

/// Draw title at top-center, stat pills stacked vertically on the right edge,
/// and a badge popout at the bottom.
#[allow(clippy::too_many_arguments)]
fn draw_tile_overlay(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rarity: usize,
    bbox: Rect,
    proj_center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let (_rarity_name, rarity_col) = RARITIES.get(rarity).copied().unwrap_or(RARITIES[0]);
    let pill_bg = Color32::from_rgba_premultiplied(20, 20, 35, 200);

    // --- Title (top center) ---
    let title_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let title_y = bbox.top() + bbox.height() * 0.07;
    let title_g =
        painter.layout_no_wrap("Shadow Drake".to_string(), title_font.clone(), text_color);
    let title_pos = Pos2::new(
        bbox.center().x - title_g.size().x / 2.0,
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

    // --- Stats (pill stack, right edge, vertically centered) ---
    let stat_font = egui::FontId::new(10.0, egui::FontFamily::Monospace);
    let icon_font = egui::FontId::new(12.0, phosphor_family());
    let pill_w = bbox.width() * 0.22;
    let pill_h = 16.0;
    let pill_gap = 3.0;
    let total_h =
        DEMO_STATS.len() as f32 * pill_h + (DEMO_STATS.len() - 1).max(0) as f32 * pill_gap;
    let stack_top = bbox.center().y - total_h / 2.0;
    let pill_right = bbox.right(); // hard right edge flush with card

    for (i, stat) in DEMO_STATS.iter().enumerate() {
        let y = stack_top + i as f32 * (pill_h + pill_gap);
        let pill_left = pill_right - pill_w;

        // Pill background: rounded left, hard right
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

        // Text (no shadow — pill bg provides contrast)
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
        // Center text within pill
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

    // --- Badge popout (extends below bottom edge) ---
    let badge_font = egui::FontId::new(11.0, phosphor_family());
    let badge_text_font = egui::FontId::new(9.0, egui::FontFamily::Monospace);
    let badge_h = 22.0;
    let badge_w = bbox.width() * 0.45;
    let badge_top = bbox.bottom() - badge_h * 0.4; // 40% overlap, 60% popout
    let badge_left = bbox.center().x - badge_w / 2.0;

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

    let badge_center_pt = Pos2::new(bbox.center().x, badge_top + badge_h / 2.0);
    let proj_badge_verts = project_points(&badge_verts, proj_center, angle_x, angle_y, perspective);
    let proj_badge_center = project_3d(badge_center_pt, proj_center, angle_x, angle_y, perspective);
    draw_colored_fan(painter, proj_badge_center, &proj_badge_verts, pill_bg);

    // Badge border
    let badge_border_r = badge_r + 1.5;
    let badge_border_cx = badge_left - 1.5 + badge_border_r;
    let badge_border_rcx = badge_right + 1.5 - badge_border_r;
    let badge_border_cy = badge_top - 1.5 + badge_border_r + 1.5; // same cy
    let mut border_verts = Vec::new();
    for i in 0..=arc_segs {
        let t = i as f32 / arc_segs as f32;
        let a = std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        border_verts.push(Pos2::new(
            badge_border_cx + a.cos() * badge_border_r,
            badge_border_cy - a.sin() * badge_border_r,
        ));
    }
    for i in 0..=arc_segs {
        let t = i as f32 / arc_segs as f32;
        let a = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::PI;
        border_verts.push(Pos2::new(
            badge_border_rcx + a.cos() * badge_border_r,
            badge_border_cy - a.sin() * badge_border_r,
        ));
    }
    let proj_border_verts =
        project_points(&border_verts, proj_center, angle_x, angle_y, perspective);
    draw_colored_ring(painter, &proj_badge_verts, &proj_border_verts, rarity_col);

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
        bbox.center().x - badge_g.size().x / 2.0,
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

#[derive(Clone, Copy)]
struct HoloParams {
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
}

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
    holo: HoloParams,
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

    // Rarity glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_rect = card_rect.expand(6.0);
        let gc = [
            glow_rect.left_top(),
            glow_rect.right_top(),
            glow_rect.right_bottom(),
            glow_rect.left_bottom(),
        ];
        let gp: Vec<Pos2> = project_points(&gc, center, ax, ay, perspective);
        draw_quad(&painter, [gp[0], gp[1], gp[2], gp[3]], glow);
    }

    // Rarity border
    let border_rect = card_rect.expand(3.0);
    let bc = [
        border_rect.left_top(),
        border_rect.right_top(),
        border_rect.right_bottom(),
        border_rect.left_bottom(),
    ];
    let bp: Vec<Pos2> = project_points(&bc, center, ax, ay, perspective);
    draw_quad(&painter, [bp[0], bp[1], bp[2], bp[3]], rarity_color(rarity));

    // Art fill
    if let Some(tex) = art_tex {
        draw_textured_quad(&painter, proj4, tex, Color32::WHITE);
    } else {
        draw_quad(&painter, proj4, Color32::from_rgb(30, 30, 48));
    }

    // Holographic overlay (Rare+)
    if rarity >= 2 {
        let (mu, mv) = if let Some(hover_pos) = response.hover_pos() {
            (
                ((hover_pos.x - card_rect.left()) / card_rect.width()).clamp(0.0, 1.0),
                ((hover_pos.y - card_rect.top()) / card_rect.height()).clamp(0.0, 1.0),
            )
        } else {
            (0.5, 0.5)
        };
        draw_holo_quad(
            &painter,
            card_rect,
            center,
            ax,
            ay,
            perspective,
            mu,
            mv,
            holo.hue_range,
            holo.shimmer_width,
            holo.shimmer_intensity,
            holo.overlay_opacity,
        );
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(ui, &painter, rarity, card_rect, center, ax, ay, perspective);
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
    holo: HoloParams,
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

    let rotation = -TAU / 4.0; // pointy-top

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_verts = regular_polygon_vertices(center, radius + 6.0, 6, rotation);
        let glow_proj = project_points(&glow_verts, center, ax, ay, perspective);
        let glow_center = project_3d(center, center, ax, ay, perspective);
        draw_colored_fan(&painter, glow_center, &glow_proj, glow);
    }

    // Border (ring between inner art and outer border)
    let art_verts = regular_polygon_vertices(center, radius, 6, rotation);
    let border_verts = regular_polygon_vertices(center, radius + 3.0, 6, rotation);
    let art_proj = project_points(&art_verts, center, ax, ay, perspective);
    let border_proj = project_points(&border_verts, center, ax, ay, perspective);
    draw_colored_ring(&painter, &art_proj, &border_proj, rarity_color(rarity));

    // Art fill
    let art_center = project_3d(center, center, ax, ay, perspective);
    if let Some(tex) = art_tex {
        draw_textured_fan(
            &painter,
            art_center,
            &art_proj,
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

    // Holographic overlay (Rare+)
    let hex_bbox = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    if rarity >= 2 {
        let (mu, mv) = if let Some(hover_pos) = response.hover_pos() {
            (
                ((hover_pos.x - hex_bbox.left()) / hex_bbox.width()).clamp(0.0, 1.0),
                ((hover_pos.y - hex_bbox.top()) / hex_bbox.height()).clamp(0.0, 1.0),
            )
        } else {
            (0.5, 0.5)
        };
        draw_holo_fan(
            &painter,
            art_center,
            &art_proj,
            hex_bbox,
            mu,
            mv,
            holo.hue_range,
            holo.shimmer_width,
            holo.shimmer_intensity,
            holo.overlay_opacity,
        );
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(ui, &painter, rarity, hex_bbox, center, ax, ay, perspective);
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
    holo: HoloParams,
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

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_verts =
            rounded_rect_vertices(center, half + 6.0, half + 6.0, corner_radius + 6.0, segs);
        let glow_proj = project_points(&glow_verts, center, ax, ay, perspective);
        let glow_center = project_3d(center, center, ax, ay, perspective);
        draw_colored_fan(&painter, glow_center, &glow_proj, glow);
    }

    // Border ring
    let art_verts = rounded_rect_vertices(center, half, half, corner_radius, segs);
    let border_verts =
        rounded_rect_vertices(center, half + 3.0, half + 3.0, corner_radius + 3.0, segs);
    let art_proj = project_points(&art_verts, center, ax, ay, perspective);
    let border_proj = project_points(&border_verts, center, ax, ay, perspective);
    draw_colored_ring(&painter, &art_proj, &border_proj, rarity_color(rarity));

    // Art fill
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

    // Holographic overlay (Rare+)
    if rarity >= 2 {
        let (mu, mv) = if let Some(hover_pos) = response.hover_pos() {
            (
                ((hover_pos.x - bbox.left()) / bbox.width()).clamp(0.0, 1.0),
                ((hover_pos.y - bbox.top()) / bbox.height()).clamp(0.0, 1.0),
            )
        } else {
            (0.5, 0.5)
        };
        draw_holo_fan(
            &painter,
            art_center,
            &art_proj,
            bbox,
            mu,
            mv,
            holo.hue_range,
            holo.shimmer_width,
            holo.shimmer_intensity,
            holo.overlay_opacity,
        );
    }

    // Overlay: title, stats, badge
    draw_tile_overlay(ui, &painter, rarity, bbox, center, ax, ay, perspective);
}

// ============================================================================
// Main show function
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut NftShapesState) {
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

    // Holo controls (only visible for Rare+)
    if state.rarity >= 2 {
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut state.hue_range, 0.0..=180.0).text("Hue Range"));
            ui.add(egui::Slider::new(&mut state.shimmer_width, 0.05..=0.5).text("Shimmer W"));
        });
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut state.shimmer_intensity, 0.0..=1.0).text("Intensity"));
            ui.add(egui::Slider::new(&mut state.overlay_opacity, 0.0..=0.5).text("Opacity"));
        });
    }

    let holo = HoloParams {
        hue_range: state.hue_range,
        shimmer_width: state.shimmer_width,
        shimmer_intensity: state.shimmer_intensity,
        overlay_opacity: state.overlay_opacity,
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
        holo,
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
        holo,
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
        holo,
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
