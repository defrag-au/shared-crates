use crate::icons::{phosphor_family, PhosphorIcon};
use egui::epaint::{Mesh, Vertex};
use egui::text::LayoutJob;
use egui::{Color32, Pos2, Rect, TextFormat, Vec2};

use super::mesh::{draw_colored_fan, draw_quad};
use super::projection::{project_3d, project_points};

// ============================================================================
// Rarity
// ============================================================================

pub const RARITIES: &[(&str, Color32)] = &[
    ("Common", Color32::from_rgb(120, 120, 140)),
    ("Uncommon", Color32::from_rgb(158, 206, 106)),
    ("Rare", Color32::from_rgb(122, 162, 247)),
    ("Epic", Color32::from_rgb(187, 154, 247)),
    ("Legendary", Color32::from_rgb(224, 175, 104)),
];

pub fn rarity_color(rarity: usize) -> Color32 {
    RARITIES.get(rarity).map_or(RARITIES[0].1, |r| r.1)
}

pub fn rarity_glow(rarity: usize) -> Option<Color32> {
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
// Shape-aware overlay positioning
// ============================================================================

/// Card mask type — controls how overlay elements position relative to the shape.
#[derive(Clone, Copy)]
pub enum CardMask {
    Square,
    Hex { radius: f32 },
    RoundedSquare { corner_radius: f32 },
}

impl CardMask {
    /// Right edge X at a given Y, relative to shape center.
    pub fn right_edge_at(&self, center: Pos2, half: f32, y: f32) -> f32 {
        match self {
            CardMask::Square => center.x + half,
            CardMask::Hex { radius } => {
                let max_half_w = radius * (std::f32::consts::PI / 6.0).cos();
                let dy = y - center.y;
                let half_r = radius * 0.5;
                if dy.abs() <= half_r {
                    center.x + max_half_w
                } else {
                    let overshoot = dy.abs() - half_r;
                    let slope_len = radius - half_r;
                    let frac = (overshoot / slope_len).clamp(0.0, 1.0);
                    center.x + max_half_w * (1.0 - frac)
                }
            }
            CardMask::RoundedSquare { corner_radius } => {
                let r = *corner_radius;
                let dy = (y - center.y).abs();
                let straight_half = half - r;
                if dy <= straight_half {
                    center.x + half
                } else {
                    let arc_dy = dy - straight_half;
                    let arc_dx = (r * r - arc_dy * arc_dy).max(0.0).sqrt();
                    center.x + straight_half + arc_dx
                }
            }
        }
    }

    fn title_y(&self, center: Pos2, half: f32) -> f32 {
        match self {
            CardMask::Square => center.y - half + half * 0.14,
            CardMask::Hex { radius } => center.y - radius * 0.72,
            CardMask::RoundedSquare { .. } => center.y - half + half * 0.14,
        }
    }

    fn badge_anchor_y(&self, center: Pos2, half: f32) -> f32 {
        match self {
            CardMask::Square => center.y + half,
            CardMask::Hex { radius } => center.y + *radius,
            CardMask::RoundedSquare { .. } => center.y + half,
        }
    }
}

// ============================================================================
// Tilted text
// ============================================================================

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
    }

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

// ============================================================================
// Stat pills
// ============================================================================

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

    // Left half-circle cap
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

    // Rectangular body
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
    let total_h = DEMO_STATS.len() as f32 * pill_h + (DEMO_STATS.len() - 1) as f32 * pill_gap;
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

// ============================================================================
// Hex overlay
// ============================================================================

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

// ============================================================================
// Tile overlay (main entry point)
// ============================================================================

/// Draw title, stat pills, and badge — layout varies by mask shape.
#[allow(clippy::too_many_arguments)]
pub fn draw_tile_overlay(
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
    crate::install_phosphor_font(ui.ctx());
    let text_color = Color32::from_rgb(220, 220, 235);
    let (_rarity_name, rarity_col) = RARITIES.get(rarity).copied().unwrap_or(RARITIES[0]);
    let pill_bg = Color32::from_rgba_premultiplied(20, 20, 35, 200);
    let center = bbox.center();
    let half = bbox.width() / 2.0;

    if matches!(mask, CardMask::Hex { .. }) {
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

    // Title
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

    // Stats
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

    // Badge popout
    let badge_font = egui::FontId::new(11.0, phosphor_family());
    let badge_text_font = egui::FontId::new(9.0, egui::FontFamily::Monospace);
    let badge_h = 22.0;
    let badge_w = bbox.width() * 0.45;
    let badge_anchor = mask.badge_anchor_y(center, half);
    let badge_top = badge_anchor - badge_h * 0.4;
    let badge_left = center.x - badge_w / 2.0;

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

    // Badge content
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
