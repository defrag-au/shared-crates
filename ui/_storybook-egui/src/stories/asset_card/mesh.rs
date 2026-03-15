use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect};
use std::f32::consts::TAU;

use super::effects::{CardEffect, EffectVertex};
use super::geometry::{cumulative_lengths, sample_path};
use super::projection::project_3d;

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

pub(super) fn draw_quad(painter: &egui::Painter, corners: [Pos2; 4], color: Color32) {
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

pub(super) fn draw_textured_quad(
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
pub(super) fn draw_colored_fan(
    painter: &egui::Painter,
    center: Pos2,
    vertices: &[Pos2],
    color: Color32,
) {
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
pub(super) fn draw_colored_ring(
    painter: &egui::Painter,
    inner: &[Pos2],
    outer: &[Pos2],
    color: Color32,
) {
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
pub(super) fn draw_spark_streak(
    painter: &egui::Painter,
    path: &[Pos2],
    phase: f32,
    color: Color32,
) {
    if path.len() < 3 {
        return;
    }
    let cum = cumulative_lengths(path);
    let total = *cum.last().unwrap();
    if total < 1.0 {
        return;
    }

    let streak_frac = 0.12; // 12% of path length
    let samples = 24;
    let glow_segs: u32 = 6;

    let [r, g, b, _] = color.to_array();

    let mut mesh = Mesh::default();

    for i in 0..samples {
        let frac = i as f32 / (samples - 1) as f32; // 0 = tail, 1 = head
        let t = (phase - streak_frac * (1.0 - frac)).rem_euclid(1.0);
        let pos = sample_path(path, &cum, t);

        // Quadratic falloff from head to tail
        let alpha = frac * frac;

        // Glow circle (larger, softer)
        let glow_radius = 8.0;
        let ga = (alpha * 0.5 * 255.0) as u8;
        let glow_color = Color32::from_rgba_unmultiplied(r, g, b, ga);
        let center_idx = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: glow_color,
        });
        for s in 0..glow_segs {
            let a = TAU * s as f32 / glow_segs as f32;
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
                center_idx + 1 + s,
                center_idx + 1 + next,
            ]);
        }

        // Core circle (smaller, bright white-tinted)
        let core_radius = 3.0;
        let ca = (alpha * 255.0) as u8;
        // Blend toward white at the head
        let wr = r.saturating_add(((255 - r) as f32 * frac) as u8);
        let wg = g.saturating_add(((255 - g) as f32 * frac) as u8);
        let wb = b.saturating_add(((255 - b) as f32 * frac) as u8);
        let core_color = Color32::from_rgba_unmultiplied(wr, wg, wb, ca);
        let center_idx2 = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos,
            uv: WHITE_UV,
            color: core_color,
        });
        for s in 0..glow_segs {
            let a = TAU * s as f32 / glow_segs as f32;
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
                center_idx2 + 1 + s,
                center_idx2 + 1 + next,
            ]);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

/// Textured triangle fan. Center vertex gets UV(0.5, 0.5).
/// Edge vertex UVs computed from angle relative to center.
pub(super) fn draw_textured_fan(
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

    mesh.vertices.push(Vertex {
        pos: center,
        uv: Pos2::new(0.5, 0.5),
        color: tint,
    });

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
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_textured_fan_rect_uv(
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

/// Effect overlay for quad shapes (square), projected through 3D.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_effect_quad(
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
pub(super) fn draw_effect_fan(
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

    for &base_idx in &tri_index_ranges {
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
