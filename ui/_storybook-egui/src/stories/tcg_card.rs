//! TCG Card story — mesh-level demos for trading card game style rendering.
//!
//! Each demo isolates a rendering technique with tunable sliders:
//! 1. Card Frame — layered regions (title, art, text, stats) as mesh quads
//! 2. Perspective Tilt — mouse-driven 3D tilt via bilinear mapping
//! 3. Holographic Effect — hue-shifted vertex colour overlay
//! 4. Card Flip — full 180° front/back flip animation
//! 5. Assembled Card — all effects combined

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Vec2};

use crate::{ACCENT, TEXT_MUTED};

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

pub struct TcgCardState {
    // Demo 1: Card Frame
    pub card_width: f32,
    pub card_height: f32,
    pub border_width: f32,
    pub corner_radius: f32,
    pub art_ratio: f32,
    pub rarity: usize,

    // Demo 2: Perspective Tilt
    pub max_tilt: f32,
    pub pinch_factor: f32,
    pub shadow_opacity: f32,
    pub tilt_ease: f32,
    pub current_tilt_x: f32,
    pub current_tilt_y: f32,

    // Demo 3: Holographic
    pub hue_range: f32,
    pub shimmer_width: f32,
    pub shimmer_intensity: f32,
    pub overlay_opacity: f32,

    // Demo 4: Card Flip
    pub flip_progress: f32,
    pub flip_animating: bool,
    pub flip_speed: f32,
    pub showing_back: bool,
}

impl Default for TcgCardState {
    fn default() -> Self {
        Self {
            card_width: 220.0,
            card_height: 320.0,
            border_width: 3.0,
            corner_radius: 8.0,
            art_ratio: 0.45,
            rarity: 2, // Rare

            max_tilt: 15.0,
            pinch_factor: 0.08,
            shadow_opacity: 0.3,
            tilt_ease: 0.1,
            current_tilt_x: 0.0,
            current_tilt_y: 0.0,

            hue_range: 60.0,
            shimmer_width: 0.15,
            shimmer_intensity: 0.4,
            overlay_opacity: 0.2,

            flip_progress: 0.0,
            flip_animating: false,
            flip_speed: 2.0,
            showing_back: false,
        }
    }
}

/// Rarity tiers with display names and colours.
const RARITIES: &[(&str, Color32)] = &[
    ("Common", Color32::from_rgb(120, 120, 140)),
    ("Uncommon", Color32::from_rgb(158, 206, 106)), // ACCENT_GREEN
    ("Rare", Color32::from_rgb(122, 162, 247)),     // ACCENT_BLUE
    ("Epic", Color32::from_rgb(187, 154, 247)),     // ACCENT_MAGENTA
    ("Legendary", Color32::from_rgb(224, 175, 104)), // ACCENT_YELLOW
];

fn rarity_color(rarity: usize) -> Color32 {
    RARITIES.get(rarity).map_or(RARITIES[0].1, |r| r.1)
}

/// Darken a colour by subtracting `amount` from each RGB channel.
fn darken(c: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        c.r().saturating_sub(amount),
        c.g().saturating_sub(amount),
        c.b().saturating_sub(amount),
        c.a(),
    )
}

/// Bilinear interpolation within a quad defined by four corners.
/// `u` goes left→right (0..1), `v` goes top→bottom (0..1).
/// Corners order: [TL, TR, BR, BL].
fn bilinear(corners: [Pos2; 4], u: f32, v: f32) -> Pos2 {
    let [tl, tr, br, bl] = corners;
    let top_x = tl.x + (tr.x - tl.x) * u;
    let top_y = tl.y + (tr.y - tl.y) * u;
    let bot_x = bl.x + (br.x - bl.x) * u;
    let bot_y = bl.y + (br.y - bl.y) * u;
    Pos2::new(top_x + (bot_x - top_x) * v, top_y + (bot_y - top_y) * v)
}

/// Stroke the outline of a quad defined by 4 corners.
fn stroke_quad(painter: &egui::Painter, corners: &[Pos2; 4], stroke: egui::Stroke) {
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
    }
}

/// Simple HSV→RGB (saturation=1, value=1).
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

/// Draw a solid-colour mesh quad from 4 corners [TL, TR, BR, BL].
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

/// Card layout regions within a card rect.
struct CardLayout {
    /// Full outer card rect
    outer: Rect,
    /// Inner area after border inset
    inner: Rect,
    /// Title bar rect
    title: Rect,
    /// Art window rect
    art: Rect,
    /// Type bar (thin separator)
    type_bar: Rect,
    /// Text box (description area)
    text_box: Rect,
    /// Stats bar at bottom
    stats: Rect,
}

impl CardLayout {
    fn compute(outer: Rect, border_width: f32, art_ratio: f32) -> Self {
        let inner = outer.shrink(border_width);
        let inner_h = inner.height();

        let title_h = 28.0;
        let type_bar_h = 4.0;
        let stats_h = 24.0;

        // Art takes art_ratio of the remaining space after fixed-height sections
        let body_h = inner_h - title_h - type_bar_h - stats_h;
        let art_h = body_h * art_ratio;
        let text_h = body_h - art_h;

        let mut y = inner.top();

        let title = Rect::from_min_size(
            Pos2::new(inner.left(), y),
            Vec2::new(inner.width(), title_h),
        );
        y += title_h;

        let art = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), art_h));
        y += art_h;

        let type_bar = Rect::from_min_size(
            Pos2::new(inner.left(), y),
            Vec2::new(inner.width(), type_bar_h),
        );
        y += type_bar_h;

        let text_box =
            Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), text_h));
        y += text_h;

        let stats = Rect::from_min_size(
            Pos2::new(inner.left(), y),
            Vec2::new(inner.width(), stats_h),
        );

        Self {
            outer,
            inner,
            title,
            art,
            type_bar,
            text_box,
            stats,
        }
    }
}

/// Draw the card frame regions using painter rects (flat, no perspective).
fn draw_card_frame_flat(
    painter: &egui::Painter,
    layout: &CardLayout,
    rarity: usize,
    border_width: f32,
    corner_radius: f32,
) {
    let rarity_col = rarity_color(rarity);
    let card_bg = Color32::from_rgb(30, 30, 48);
    let title_bg = Color32::from_rgb(35, 35, 55);
    let art_bg = Color32::from_rgb(50, 55, 75);
    let type_bar_bg = darken(rarity_col, 80);
    let text_bg = Color32::from_rgb(28, 28, 44);
    let stats_bg = Color32::from_rgb(32, 32, 50);

    // Outer card with rarity border
    painter.rect_filled(layout.outer, corner_radius, rarity_col);
    // Inner background
    painter.rect_filled(
        layout.inner,
        (corner_radius - border_width).max(0.0),
        card_bg,
    );

    // Card regions — top/bottom sections get corner rounding to match outer border
    let inner_r = (corner_radius - border_width).max(0.0) as u8;
    let top_rounding = egui::CornerRadius {
        nw: inner_r,
        ne: inner_r,
        sw: 0,
        se: 0,
    };
    let bot_rounding = egui::CornerRadius {
        nw: 0,
        ne: 0,
        sw: inner_r,
        se: inner_r,
    };
    painter.rect_filled(layout.title, top_rounding, title_bg);
    painter.rect_filled(layout.art, 0.0, art_bg);
    painter.rect_filled(layout.type_bar, 0.0, type_bar_bg);
    painter.rect_filled(layout.text_box, 0.0, text_bg);
    painter.rect_filled(layout.stats, bot_rounding, stats_bg);
}

/// Draw text labels within card regions using clip rects (flat, no perspective).
fn draw_card_labels_flat(painter: &egui::Painter, layout: &CardLayout, rarity: usize) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let muted_color = Color32::from_rgb(140, 140, 160);

    // Title
    let title_galley = painter.layout_no_wrap(
        "Shadow Drake".to_string(),
        egui::FontId::new(16.0, egui::FontFamily::Monospace),
        text_color,
    );
    let title_pos = Pos2::new(
        layout.title.left() + 8.0,
        layout.title.center().y - title_galley.size().y / 2.0,
    );
    painter.galley(title_pos, title_galley, Color32::TRANSPARENT);

    // Art placeholder text
    let art_galley = painter.layout_no_wrap(
        "[ Art ]".to_string(),
        egui::FontId::new(14.0, egui::FontFamily::Monospace),
        muted_color,
    );
    let art_pos = Pos2::new(
        layout.art.center().x - art_galley.size().x / 2.0,
        layout.art.center().y - art_galley.size().y / 2.0,
    );
    painter.galley(art_pos, art_galley, Color32::TRANSPARENT);

    // Type line
    let type_text = RARITIES.get(rarity).map_or("Common", |r| r.0);
    let type_galley = painter.layout_no_wrap(
        format!("Dragon  —  {type_text}"),
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        muted_color,
    );
    // Type bar is thin — draw centered vertically in the text_box top area instead
    let type_pos = Pos2::new(
        layout.text_box.left() + 8.0,
        layout.type_bar.center().y - type_galley.size().y / 2.0 + 8.0,
    );
    painter.galley(type_pos, type_galley, Color32::TRANSPARENT);

    // Description text
    let desc_galley = painter.layout(
        "When this creature enters\nthe battlefield, deal 3\ndamage to target player.".to_string(),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        muted_color,
        layout.text_box.width() - 16.0,
    );
    let desc_pos = Pos2::new(layout.text_box.left() + 8.0, layout.text_box.top() + 14.0);
    let clipped = painter.clip_rect().intersect(layout.text_box);
    if clipped.is_positive() {
        let sub = painter.with_clip_rect(clipped);
        sub.galley(desc_pos, desc_galley, Color32::TRANSPARENT);
    }

    // Stats (ATK / DEF)
    let stats_galley = painter.layout_no_wrap(
        "ATK 5  /  DEF 3".to_string(),
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
        text_color,
    );
    let stats_pos = Pos2::new(
        layout.stats.center().x - stats_galley.size().x / 2.0,
        layout.stats.center().y - stats_galley.size().y / 2.0,
    );
    painter.galley(stats_pos, stats_galley, Color32::TRANSPARENT);
}

/// Compute perspective-tilted quad corners from a rect and tilt amounts.
///
/// `tilt_x` and `tilt_y` are in the range -1.0..1.0.
/// Positive tilt_x = right side closer to viewer (right edge expands).
/// Positive tilt_y = bottom closer to viewer (bottom edge expands).
fn tilt_corners(rect: Rect, tilt_x: f32, tilt_y: f32, pinch_factor: f32) -> [Pos2; 4] {
    let w = rect.width();
    let h = rect.height();
    let px = w * pinch_factor * tilt_x;
    let py = h * pinch_factor * tilt_y;

    [
        // TL: moves right when tilting right (+px), moves down when tilting down (+py)
        Pos2::new(rect.left() + px.max(0.0), rect.top() + py.max(0.0)),
        // TR: moves left when tilting left (-px means px<0, so -px.min(0.0) = |px|)
        Pos2::new(rect.right() + px.min(0.0), rect.top() - py.min(0.0)),
        // BR
        Pos2::new(rect.right() - px.max(0.0), rect.bottom() - py.max(0.0)),
        // BL
        Pos2::new(rect.left() - px.min(0.0), rect.bottom() + py.min(0.0)),
    ]
}

/// Draw the card frame regions mapped into a perspective quad via bilinear interpolation.
fn draw_card_frame_perspective(
    painter: &egui::Painter,
    layout: &CardLayout,
    corners: [Pos2; 4],
    rarity: usize,
) {
    let rarity_col = rarity_color(rarity);
    let card_bg = Color32::from_rgb(30, 30, 48);
    let title_bg = Color32::from_rgb(35, 35, 55);
    let art_bg = Color32::from_rgb(50, 55, 75);
    let type_bar_bg = darken(rarity_col, 80);
    let text_bg = Color32::from_rgb(28, 28, 44);
    let stats_bg = Color32::from_rgb(32, 32, 50);

    let outer = layout.outer;
    let ow = outer.width();
    let oh = outer.height();

    // Helper: map a sub-rect into the perspective quad
    let map_rect = |r: Rect| -> [Pos2; 4] {
        let u0 = (r.left() - outer.left()) / ow;
        let u1 = (r.right() - outer.left()) / ow;
        let v0 = (r.top() - outer.top()) / oh;
        let v1 = (r.bottom() - outer.top()) / oh;
        [
            bilinear(corners, u0, v0),
            bilinear(corners, u1, v0),
            bilinear(corners, u1, v1),
            bilinear(corners, u0, v1),
        ]
    };

    // Full card (rarity border colour)
    draw_quad(painter, corners, rarity_col);

    // Inner background
    draw_quad(painter, map_rect(layout.inner), card_bg);

    // Regions
    draw_quad(painter, map_rect(layout.title), title_bg);
    draw_quad(painter, map_rect(layout.art), art_bg);
    draw_quad(painter, map_rect(layout.type_bar), type_bar_bg);
    draw_quad(painter, map_rect(layout.text_box), text_bg);
    draw_quad(painter, map_rect(layout.stats), stats_bg);
}

/// Draw text mapped into perspective quad using galley mesh extraction.
fn draw_card_text_perspective(
    ui: &egui::Ui,
    painter: &egui::Painter,
    layout: &CardLayout,
    corners: [Pos2; 4],
    rarity: usize,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let muted_color = Color32::from_rgb(140, 140, 160);
    let outer = layout.outer;

    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

    // Helper: draw a galley mapped into the perspective quad.
    // `text_pos` is where the text would be drawn in flat screen space.
    let draw_mapped = |galley: &std::sync::Arc<egui::Galley>, text_pos: Pos2| {
        let mut text_mesh = Mesh::with_texture(egui::TextureId::default());

        for placed_row in &galley.rows {
            let row_offset = placed_row.pos;
            let row_mesh = &placed_row.row.visuals.mesh;
            let idx_offset = text_mesh.vertices.len() as u32;

            for vertex in &row_mesh.vertices {
                let abs_x = text_pos.x + row_offset.x + vertex.pos.x;
                let abs_y = text_pos.y + row_offset.y + vertex.pos.y;

                let u = (abs_x - outer.left()) / outer.width();
                let v = (abs_y - outer.top()) / outer.height();

                let screen_pos = bilinear(corners, u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                let norm_uv = Pos2::new(vertex.uv.x * uv_norm.x, vertex.uv.y * uv_norm.y);

                text_mesh.vertices.push(Vertex {
                    pos: screen_pos,
                    uv: norm_uv,
                    color: vertex.color,
                });
            }

            for &idx in &row_mesh.indices {
                text_mesh.indices.push(idx + idx_offset);
            }
        }

        painter.add(egui::Shape::mesh(text_mesh));
    };

    // Title
    let title_galley = painter.layout_no_wrap(
        "Shadow Drake".to_string(),
        egui::FontId::new(16.0, egui::FontFamily::Monospace),
        text_color,
    );
    let title_pos = Pos2::new(
        layout.title.left() + 8.0,
        layout.title.center().y - title_galley.size().y / 2.0,
    );
    draw_mapped(&title_galley, title_pos);

    // Art placeholder
    let art_galley = painter.layout_no_wrap(
        "[ Art ]".to_string(),
        egui::FontId::new(14.0, egui::FontFamily::Monospace),
        muted_color,
    );
    let art_pos = Pos2::new(
        layout.art.center().x - art_galley.size().x / 2.0,
        layout.art.center().y - art_galley.size().y / 2.0,
    );
    draw_mapped(&art_galley, art_pos);

    // Type line
    let type_text = RARITIES.get(rarity).map_or("Common", |r| r.0);
    let type_galley = painter.layout_no_wrap(
        format!("Dragon  —  {type_text}"),
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        muted_color,
    );
    let type_pos = Pos2::new(
        layout.text_box.left() + 8.0,
        layout.type_bar.center().y - type_galley.size().y / 2.0 + 8.0,
    );
    draw_mapped(&type_galley, type_pos);

    // Description (wrapping)
    let desc_galley = painter.layout(
        "When this creature enters\nthe battlefield, deal 3\ndamage to target player.".to_string(),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        muted_color,
        layout.text_box.width() - 16.0,
    );
    let desc_pos = Pos2::new(layout.text_box.left() + 8.0, layout.text_box.top() + 14.0);
    draw_mapped(&desc_galley, desc_pos);

    // Stats
    let stats_galley = painter.layout_no_wrap(
        "ATK 5  /  DEF 3".to_string(),
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
        text_color,
    );
    let stats_pos = Pos2::new(
        layout.stats.center().x - stats_galley.size().x / 2.0,
        layout.stats.center().y - stats_galley.size().y / 2.0,
    );
    draw_mapped(&stats_galley, stats_pos);
}

/// Draw a holographic overlay on top of the card.
///
/// The overlay is a semi-transparent mesh with hue-shifted vertex colours
/// that respond to `mouse_u` and `mouse_v` (normalised mouse position over the card).
fn draw_holo_overlay(
    painter: &egui::Painter,
    corners: [Pos2; 4],
    mouse_u: f32,
    mouse_v: f32,
    hue_range: f32,
    shimmer_width: f32,
    shimmer_intensity: f32,
    overlay_opacity: f32,
) {
    // Subdivide the card into a grid for smooth colour interpolation
    let cols = 8_u32;
    let rows = 12_u32;

    let base_hue = mouse_u * 360.0;

    let mut mesh = Mesh::default();

    for row in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = row as f32 / rows as f32;
            let pos = bilinear(corners, u, v);

            // Hue varies across the card surface
            let hue = base_hue + (u - 0.5) * hue_range + (v - 0.5) * hue_range * 0.5;
            let mut color = hue_to_rgb(hue);

            // Shimmer band: bright strip that follows the mouse
            let dist_to_mouse = ((u - mouse_u).powi(2) + (v - mouse_v).powi(2)).sqrt();
            let shimmer = if dist_to_mouse < shimmer_width {
                let t = 1.0 - dist_to_mouse / shimmer_width;
                t * t * shimmer_intensity
            } else {
                0.0
            };

            // Combine: base overlay + shimmer boost
            let alpha = ((overlay_opacity + shimmer) * 255.0).min(255.0) as u8;
            color = Color32::from_rgba_premultiplied(
                (color.r() as f32 * alpha as f32 / 255.0) as u8,
                (color.g() as f32 * alpha as f32 / 255.0) as u8,
                (color.b() as f32 * alpha as f32 / 255.0) as u8,
                alpha,
            );

            mesh.vertices.push(Vertex {
                pos,
                uv: WHITE_UV,
                color,
            });
        }
    }

    // Generate triangle indices for the grid
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

/// Draw a geometric pattern for the card back.
fn draw_card_back(painter: &egui::Painter, corners: [Pos2; 4], rarity: usize) {
    let rarity_col = rarity_color(rarity);
    let bg = Color32::from_rgb(20, 20, 38);

    // Fill background
    draw_quad(painter, corners, bg);

    // Diamond grid pattern
    let grid_cols = 6_u32;
    let grid_rows = 8_u32;
    let diamond_color = darken(rarity_col, 120);

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cu = (col as f32 + 0.5) / grid_cols as f32;
            let cv = (row as f32 + 0.5) / grid_rows as f32;
            let du = 0.3 / grid_cols as f32;
            let dv = 0.3 / grid_rows as f32;

            // Diamond: top, right, bottom, left
            let diamond = [
                bilinear(corners, cu, cv - dv), // top
                bilinear(corners, cu + du, cv), // right
                bilinear(corners, cu, cv + dv), // bottom
                bilinear(corners, cu - du, cv), // left
            ];
            draw_quad(painter, diamond, diamond_color);
        }
    }

    // Center emblem: larger diamond
    let center = [
        bilinear(corners, 0.5, 0.35),
        bilinear(corners, 0.65, 0.5),
        bilinear(corners, 0.5, 0.65),
        bilinear(corners, 0.35, 0.5),
    ];
    draw_quad(painter, center, rarity_col);
    let inner_emblem = [
        bilinear(corners, 0.5, 0.40),
        bilinear(corners, 0.60, 0.5),
        bilinear(corners, 0.5, 0.60),
        bilinear(corners, 0.40, 0.5),
    ];
    draw_quad(painter, inner_emblem, bg);
}

pub fn show(ui: &mut egui::Ui, state: &mut TcgCardState) {
    let dt = ui.input(|i| i.stable_dt).min(0.1);

    // --- 1. Card Frame ---
    ui.label(egui::RichText::new("1. Card Frame").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Layered card regions as mesh quads: title bar, art window, type bar, \
             text box, stats bar. Rarity colour drives the border.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.card_width, 140.0..=300.0).text("Width"));
        ui.add(egui::Slider::new(&mut state.card_height, 200.0..=450.0).text("Height"));
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.border_width, 1.0..=8.0).text("Border"));
        ui.add(egui::Slider::new(&mut state.corner_radius, 0.0..=20.0).text("Radius"));
    });
    ui.add(egui::Slider::new(&mut state.art_ratio, 0.2..=0.7).text("Art Ratio"));

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
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(state.card_width + 20.0, state.card_height + 20.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    let card_rect = Rect::from_center_size(
        rect.center(),
        Vec2::new(state.card_width, state.card_height),
    );
    let layout = CardLayout::compute(card_rect, state.border_width, state.art_ratio);
    draw_card_frame_flat(
        &painter,
        &layout,
        state.rarity,
        state.border_width,
        state.corner_radius,
    );
    draw_card_labels_flat(&painter, &layout, state.rarity);

    ui.add_space(16.0);

    // --- 2. Perspective Tilt ---
    ui.label(
        egui::RichText::new("2. Perspective Tilt (Mouse-Driven)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Card tilts toward the mouse cursor. All regions bilinearly mapped into \
             the perspective quad. Hover over the card to see the effect.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.max_tilt, 0.0..=30.0).text("Max Tilt"));
        ui.add(egui::Slider::new(&mut state.pinch_factor, 0.0..=0.2).text("Pinch"));
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.tilt_ease, 0.01..=0.5).text("Ease"));
        ui.add(egui::Slider::new(&mut state.shadow_opacity, 0.0..=1.0).text("Shadow"));
    });
    ui.add_space(4.0);

    let (rect2, response2) = ui.allocate_exact_size(
        Vec2::new(state.card_width + 60.0, state.card_height + 60.0),
        egui::Sense::hover(),
    );
    let painter2 = ui.painter_at(rect2);

    let card_rect2 = Rect::from_center_size(
        rect2.center(),
        Vec2::new(state.card_width, state.card_height),
    );
    let layout2 = CardLayout::compute(card_rect2, state.border_width, state.art_ratio);

    // Compute target tilt from mouse position
    let (target_x, target_y) = if let Some(hover_pos) = response2.hover_pos() {
        let rel_x = (hover_pos.x - card_rect2.center().x) / (card_rect2.width() / 2.0);
        let rel_y = (hover_pos.y - card_rect2.center().y) / (card_rect2.height() / 2.0);
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };

    // Smooth lerp toward target
    state.current_tilt_x += (target_x - state.current_tilt_x) * state.tilt_ease;
    state.current_tilt_y += (target_y - state.current_tilt_y) * state.tilt_ease;

    if (state.current_tilt_x.abs() > 0.001) || (state.current_tilt_y.abs() > 0.001) {
        ui.ctx().request_repaint();
    }

    let corners2 = tilt_corners(
        card_rect2,
        state.current_tilt_x,
        state.current_tilt_y,
        state.pinch_factor,
    );

    // Shadow behind the card
    if state.shadow_opacity > 0.0 {
        let shadow_offset = Vec2::new(state.current_tilt_x * 8.0, state.current_tilt_y * 8.0 + 4.0);
        let shadow_corners =
            corners2.map(|p| Pos2::new(p.x + shadow_offset.x, p.y + shadow_offset.y));
        let shadow_alpha = (state.shadow_opacity * 80.0) as u8;
        draw_quad(
            &painter2,
            shadow_corners,
            Color32::from_rgba_premultiplied(0, 0, 0, shadow_alpha),
        );
    }

    draw_card_frame_perspective(&painter2, &layout2, corners2, state.rarity);
    draw_card_text_perspective(ui, &painter2, &layout2, corners2, state.rarity);

    ui.add_space(16.0);

    // --- 3. Holographic / Foil Effect ---
    ui.label(
        egui::RichText::new("3. Holographic / Foil Effect")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Hue-shifted vertex colour overlay with a shimmer band that tracks the cursor. \
             Subdivided mesh grid for smooth colour interpolation.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.hue_range, 0.0..=180.0).text("Hue Range"));
        ui.add(egui::Slider::new(&mut state.shimmer_width, 0.05..=0.5).text("Shimmer W"));
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.shimmer_intensity, 0.0..=1.0).text("Intensity"));
        ui.add(egui::Slider::new(&mut state.overlay_opacity, 0.0..=0.5).text("Opacity"));
    });
    ui.add_space(4.0);

    let (rect3, response3) = ui.allocate_exact_size(
        Vec2::new(state.card_width + 20.0, state.card_height + 20.0),
        egui::Sense::hover(),
    );
    let painter3 = ui.painter_at(rect3);

    let card_rect3 = Rect::from_center_size(
        rect3.center(),
        Vec2::new(state.card_width, state.card_height),
    );
    let layout3 = CardLayout::compute(card_rect3, state.border_width, state.art_ratio);
    let corners3 = [
        card_rect3.left_top(),
        card_rect3.right_top(),
        card_rect3.right_bottom(),
        card_rect3.left_bottom(),
    ];

    // Draw base card
    draw_card_frame_flat(
        &painter3,
        &layout3,
        state.rarity,
        state.border_width,
        state.corner_radius,
    );
    draw_card_labels_flat(&painter3, &layout3, state.rarity);

    // Mouse UV for holo
    let (mouse_u, mouse_v) = if let Some(hover_pos) = response3.hover_pos() {
        let u = ((hover_pos.x - card_rect3.left()) / card_rect3.width()).clamp(0.0, 1.0);
        let v = ((hover_pos.y - card_rect3.top()) / card_rect3.height()).clamp(0.0, 1.0);
        ui.ctx().request_repaint();
        (u, v)
    } else {
        (0.5, 0.5)
    };

    draw_holo_overlay(
        &painter3,
        corners3,
        mouse_u,
        mouse_v,
        state.hue_range,
        state.shimmer_width,
        state.shimmer_intensity,
        state.overlay_opacity,
    );

    ui.add_space(16.0);

    // --- 4. Card Flip ---
    ui.label(
        egui::RichText::new("4. Card Flip (Front / Back)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Full 180° horizontal flip. Phase 1: card narrows to edge-on. \
             Phase 2: card widens showing back face. Click to flip.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    // Advance flip animation
    if state.flip_animating {
        state.flip_progress += dt * state.flip_speed;
        if state.flip_progress >= 1.0 {
            state.flip_progress = 1.0;
            state.flip_animating = false;
            state.showing_back = !state.showing_back;
            state.flip_progress = 0.0;
        }
        ui.ctx().request_repaint();
    }

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.flip_speed, 0.5..=5.0).text("Speed"));
        if ui.button("Flip").clicked() && !state.flip_animating {
            state.flip_animating = true;
            state.flip_progress = 0.0;
        }
        if ui.button("Reset").clicked() {
            state.flip_animating = false;
            state.flip_progress = 0.0;
            state.showing_back = false;
        }
    });
    if !state.flip_animating {
        ui.add(egui::Slider::new(&mut state.flip_progress, 0.0..=1.0).text("Progress"));
    }
    ui.add_space(4.0);

    let (rect4, response4) = ui.allocate_exact_size(
        Vec2::new(state.card_width + 40.0, state.card_height + 20.0),
        egui::Sense::click(),
    );
    let painter4 = ui.painter_at(rect4);

    if response4.clicked() && !state.flip_animating {
        state.flip_animating = true;
        state.flip_progress = 0.0;
    }

    let card_rect4 = Rect::from_center_size(
        rect4.center(),
        Vec2::new(state.card_width, state.card_height),
    );
    let layout4 = CardLayout::compute(card_rect4, state.border_width, state.art_ratio);

    let p = state.flip_progress;
    // Ease-out quadratic within each phase
    let (showing_front, width_fraction) = if p < 0.5 {
        // Phase 1: front face narrowing
        let phase_t = p * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        let w = 1.0 - eased; // 1.0 → 0.0
        (!state.showing_back, w)
    } else {
        // Phase 2: back face widening
        let phase_t = (p - 0.5) * 2.0;
        let eased = 1.0 - (1.0 - phase_t) * (1.0 - phase_t);
        (state.showing_back, eased) // 0.0 → 1.0
    };

    let cx = card_rect4.center().x;
    let half_w = state.card_width / 2.0 * width_fraction;
    let flip_corners = [
        Pos2::new(cx - half_w, card_rect4.top()),
        Pos2::new(cx + half_w, card_rect4.top()),
        Pos2::new(cx + half_w, card_rect4.bottom()),
        Pos2::new(cx - half_w, card_rect4.bottom()),
    ];

    if width_fraction > 0.01 {
        if showing_front {
            draw_card_frame_perspective(&painter4, &layout4, flip_corners, state.rarity);
            draw_card_text_perspective(ui, &painter4, &layout4, flip_corners, state.rarity);
        } else {
            draw_card_back(&painter4, flip_corners, state.rarity);
        }
        // Border around the flipped card
        let border_col = rarity_color(state.rarity);
        stroke_quad(
            &painter4,
            &flip_corners,
            egui::Stroke::new(state.border_width, border_col),
        );
    }

    // Phase indicator
    let face_label = if showing_front { "Front" } else { "Back" };
    let face_state = if state.showing_back { "Back" } else { "Front" };
    ui.label(
        egui::RichText::new(format!(
            "Showing: {face_label} (base: {face_state}, width: {:.0}%)",
            width_fraction * 100.0
        ))
        .color(TEXT_MUTED)
        .small(),
    );

    ui.add_space(16.0);

    // --- 5. Assembled Card ---
    ui.label(
        egui::RichText::new("5. Assembled Card")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "All effects combined: perspective tilt, holographic overlay (Rare+), \
             click to flip. This is the visual spec for the eventual widget.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let (rect5, response5) = ui.allocate_exact_size(
        Vec2::new(state.card_width + 80.0, state.card_height + 80.0),
        egui::Sense::click_and_drag(),
    );
    let painter5 = ui.painter_at(rect5);

    let card_rect5 = Rect::from_center_size(
        rect5.center(),
        Vec2::new(state.card_width, state.card_height),
    );
    let layout5 = CardLayout::compute(card_rect5, state.border_width, state.art_ratio);

    // Mouse tilt (reuse same smoothed state since demo 2 lerps continuously)
    let (target5_x, target5_y) = if let Some(hover_pos) = response5.hover_pos() {
        let rel_x = (hover_pos.x - card_rect5.center().x) / (card_rect5.width() / 2.0);
        let rel_y = (hover_pos.y - card_rect5.center().y) / (card_rect5.height() / 2.0);
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };

    // Use independent tilt state for demo 5 to avoid interference with demo 2
    // (We reuse the same fields — they'll settle quickly via lerp)
    state.current_tilt_x += (target5_x - state.current_tilt_x) * state.tilt_ease;
    state.current_tilt_y += (target5_y - state.current_tilt_y) * state.tilt_ease;

    if response5.hovered() {
        ui.ctx().request_repaint();
    }

    let corners5 = tilt_corners(
        card_rect5,
        state.current_tilt_x,
        state.current_tilt_y,
        state.pinch_factor,
    );

    // Shadow
    if state.shadow_opacity > 0.0 {
        let shadow_offset = Vec2::new(state.current_tilt_x * 8.0, state.current_tilt_y * 8.0 + 4.0);
        let shadow_corners =
            corners5.map(|p| Pos2::new(p.x + shadow_offset.x, p.y + shadow_offset.y));
        let shadow_alpha = (state.shadow_opacity * 80.0) as u8;
        draw_quad(
            &painter5,
            shadow_corners,
            Color32::from_rgba_premultiplied(0, 0, 0, shadow_alpha),
        );
    }

    // Draw card (front or back based on showing_back state)
    if state.showing_back && !state.flip_animating {
        draw_card_back(&painter5, corners5, state.rarity);
        stroke_quad(
            &painter5,
            &corners5,
            egui::Stroke::new(state.border_width, rarity_color(state.rarity)),
        );
    } else {
        draw_card_frame_perspective(&painter5, &layout5, corners5, state.rarity);
        draw_card_text_perspective(ui, &painter5, &layout5, corners5, state.rarity);

        // Holographic overlay for Rare and above
        if state.rarity >= 2 {
            let (mu, mv) = if let Some(hover_pos) = response5.hover_pos() {
                let u = ((hover_pos.x - card_rect5.left()) / card_rect5.width()).clamp(0.0, 1.0);
                let v = ((hover_pos.y - card_rect5.top()) / card_rect5.height()).clamp(0.0, 1.0);
                (u, v)
            } else {
                (0.5, 0.5)
            };
            draw_holo_overlay(
                &painter5,
                corners5,
                mu,
                mv,
                state.hue_range,
                state.shimmer_width,
                state.shimmer_intensity,
                state.overlay_opacity,
            );
        }
    }

    // Click to flip
    if response5.clicked() {
        state.showing_back = !state.showing_back;
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Key patterns:").color(ACCENT).strong());
    ui.label("- Card frame: layered rects for title, art, type, text, stats regions");
    ui.label("- Perspective: tilt_corners() computes pinched quad from mouse offset");
    ui.label("- All regions bilinearly mapped into the perspective quad");
    ui.label("- Text: galley mesh vertices extracted and mapped into perspective space");
    ui.label("- Holographic: subdivided mesh grid with hue-shifted vertex colours");
    ui.label("- Shimmer: bright band follows cursor via distance-based falloff");
    ui.label("- Flip: horizontal width scaling with ease-out quadratic per phase");
}
