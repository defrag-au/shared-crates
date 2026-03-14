//! NFT Shape Experiments — exploring alternatives to the card metaphor.
//!
//! Four shape demos rendered with the same IIIF art, rarity border, and stat
//! overlay so they're directly comparable:
//! 1. Square Tile — natural 1:1 fit, art-first
//! 2. Hex Tile — tessellatable, game-board feel
//! 3. Circle / Medallion — PFP culture, iconic
//! 4. Rounded Square — modern app-icon aesthetic
//!
//! Tilt uses proper 3D perspective projection: each vertex is rotated around
//! X/Y axes then projected with perspective division.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Vec2};
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
    // Per-demo tilt state
    tilts: [TiltState; 4],
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
            tilts: [TiltState::default(); 4],
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
// Tilted text
// ============================================================================

/// Draw text projected through 3D perspective.
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
    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

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

/// Draw title/stats/rarity labels projected through 3D perspective.
#[allow(clippy::too_many_arguments)]
fn draw_projected_labels(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rarity: usize,
    label_center_x: f32,
    title_y: f32,
    stats_y: f32,
    rarity_y: f32,
    proj_center: Pos2,
    angle_x: f32,
    angle_y: f32,
    perspective: f32,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let stats_color = Color32::from_rgb(200, 200, 220);
    let (rarity_name, rarity_col) = RARITIES.get(rarity).copied().unwrap_or(RARITIES[0]);

    let title_font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    let stats_font = egui::FontId::new(11.0, egui::FontFamily::Monospace);
    let rarity_font = egui::FontId::new(9.0, egui::FontFamily::Monospace);

    let title_g =
        painter.layout_no_wrap("Shadow Drake".to_string(), title_font.clone(), text_color);
    let title_pos = Pos2::new(
        label_center_x - title_g.size().x / 2.0,
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

    let stats_g = painter.layout_no_wrap(
        "ATK 5  /  DEF 3".to_string(),
        stats_font.clone(),
        stats_color,
    );
    let stats_pos = Pos2::new(
        label_center_x - stats_g.size().x / 2.0,
        stats_y - stats_g.size().y / 2.0,
    );
    draw_projected_text(
        ui,
        painter,
        "ATK 5  /  DEF 3",
        stats_font,
        stats_color,
        stats_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
    );

    let rarity_g = painter.layout_no_wrap(rarity_name.to_string(), rarity_font.clone(), rarity_col);
    let rarity_pos = Pos2::new(
        label_center_x - rarity_g.size().x / 2.0,
        rarity_y - rarity_g.size().y / 2.0,
    );
    draw_projected_text(
        ui,
        painter,
        rarity_name,
        rarity_font,
        rarity_col,
        rarity_pos,
        proj_center,
        angle_x,
        angle_y,
        perspective,
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

    // Title overlay band (top 14%)
    let band_h = size * 0.14;
    let tb = [
        Pos2::new(card_rect.left(), card_rect.top()),
        Pos2::new(card_rect.right(), card_rect.top()),
        Pos2::new(card_rect.right(), card_rect.top() + band_h),
        Pos2::new(card_rect.left(), card_rect.top() + band_h),
    ];
    let tbp: Vec<Pos2> = project_points(&tb, center, ax, ay, perspective);
    draw_quad(
        &painter,
        [tbp[0], tbp[1], tbp[2], tbp[3]],
        Color32::from_rgba_premultiplied(10, 10, 20, 160),
    );

    // Stats overlay band (bottom 18%)
    let stats_h = size * 0.18;
    let sb = [
        Pos2::new(card_rect.left(), card_rect.bottom() - stats_h),
        Pos2::new(card_rect.right(), card_rect.bottom() - stats_h),
        Pos2::new(card_rect.right(), card_rect.bottom()),
        Pos2::new(card_rect.left(), card_rect.bottom()),
    ];
    let sbp: Vec<Pos2> = project_points(&sb, center, ax, ay, perspective);
    draw_quad(
        &painter,
        [sbp[0], sbp[1], sbp[2], sbp[3]],
        Color32::from_rgba_premultiplied(10, 10, 20, 160),
    );

    // Text labels
    draw_projected_labels(
        ui,
        &painter,
        rarity,
        card_rect.center().x,
        card_rect.top() + band_h / 2.0,
        card_rect.bottom() - stats_h * 0.55,
        card_rect.bottom() - stats_h * 0.2,
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

    // Labels below
    let below_y = center.y + radius + 8.0;
    draw_projected_labels(
        ui,
        &painter,
        rarity,
        center.x,
        below_y + 7.0,
        below_y + 25.0,
        below_y + 41.0,
        center,
        ax,
        ay,
        perspective,
    );
}

#[allow(clippy::too_many_arguments)]
fn demo_circle(
    ui: &mut egui::Ui,
    size: f32,
    rarity: usize,
    art_tex: Option<egui::TextureId>,
    tilt: &mut TiltState,
    tilt_ease: f32,
    max_tilt: f32,
    perspective: f32,
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

    let segments = 48;

    // Glow
    if let Some(glow) = rarity_glow(rarity) {
        let glow_verts = regular_polygon_vertices(center, radius + 6.0, segments, 0.0);
        let glow_proj = project_points(&glow_verts, center, ax, ay, perspective);
        let glow_center = project_3d(center, center, ax, ay, perspective);
        draw_colored_fan(&painter, glow_center, &glow_proj, glow);
    }

    // Border ring
    let art_verts = regular_polygon_vertices(center, radius, segments, 0.0);
    let border_verts = regular_polygon_vertices(center, radius + 3.0, segments, 0.0);
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

    // Labels below
    let below_y = center.y + radius + 8.0;
    draw_projected_labels(
        ui,
        &painter,
        rarity,
        center.x,
        below_y + 7.0,
        below_y + 25.0,
        below_y + 41.0,
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

    // Labels below the shape (overlay bands don't work with rounded corners)
    let label_x = project_3d(
        Pos2::new(center.x, bbox.bottom() + 8.0),
        center,
        ax,
        ay,
        perspective,
    );
    painter.text(
        label_x,
        egui::Align2::CENTER_TOP,
        "Shadow Drake",
        egui::FontId::proportional(14.0),
        Color32::WHITE,
    );
    painter.text(
        Pos2::new(label_x.x, label_x.y + 18.0),
        egui::Align2::CENTER_TOP,
        format!("ATK 5 / DEF 3  —  {}", RARITIES[rarity].0),
        egui::FontId::proportional(12.0),
        Color32::LIGHT_GRAY,
    );
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
            "Natural 1:1 fit. Art fills the square with overlay bands for title/stats.",
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
    );
    ui.add_space(16.0);

    // --- 2. Hex Tile ---
    ui.label(egui::RichText::new("2. Hex Tile").color(ACCENT).strong());
    ui.label(
        egui::RichText::new("Pointy-top hexagon. Tessellates for game boards. Labels below shape.")
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
    );
    ui.add_space(16.0);

    // --- 3. Circle / Medallion ---
    ui.label(
        egui::RichText::new("3. Circle / Medallion")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Clean, iconic. Fits PFP culture. Labels below shape.")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);
    demo_circle(
        ui,
        state.size,
        state.rarity,
        art_texture,
        &mut state.tilts[2],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
    );
    ui.add_space(16.0);

    // --- 4. Rounded Square ---
    ui.label(
        egui::RichText::new("4. Rounded Square")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Modern app-icon aesthetic. Generous corner radius with overlay bands.",
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
        &mut state.tilts[3],
        state.tilt_ease,
        15.0,
        state.perspective_distance,
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
    ui.label("- Circle: cleanest silhouette, loses corner art detail");
    ui.label("- Rounded Square: modern feel, good compromise");
}
