//! TCG Card story — template-driven card rendering with colour-keyed masks.
//!
//! Each demo isolates a rendering technique with tunable sliders:
//! 1. Card Frame — template-driven layout with art background and frame overlay
//! 2. Perspective Tilt — mouse-driven 3D tilt via bilinear mapping
//! 3. Holographic Effect — hue-shifted vertex colour overlay
//! 4. Card Flip — full 180° front/back flip animation
//! 5. Assembled Card — all effects combined
//!
//! The card layout is driven by two template PNGs:
//! - **frame.png** — decorative border overlay with transparency
//! - **mask.png** — colour-keyed regions defining functional areas

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Vec2};

use crate::{ACCENT, TEXT_MUTED};

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

/// IIIF test image: Pirate758 NFT
const IIIF_ART_URL: &str = "https://iiif.hodlcroft.com/iiif/3/b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465373538/full/400,/0/default.jpg";

/// Template asset URLs (served by Trunk from assets/ dir)
const FRAME_URL: &str = "./assets/templates/default/frame.png";
const MASK_URL: &str = "./assets/templates/default/mask.png";

// ============================================================================
// Colour key convention for mask regions
// ============================================================================

/// Functional regions extracted from a colour-keyed mask image.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum CardRegion {
    Title,
    Art,
    Description,
    Stats,
    TypeLine,
}

impl CardRegion {
    /// Match a pixel's RGB to a known region colour.
    fn from_rgb(r: u8, g: u8, b: u8) -> Option<Self> {
        match (r, g, b) {
            (255, 0, 0) => Some(Self::Title),
            (0, 255, 0) => Some(Self::Art),
            (0, 0, 255) => Some(Self::Description),
            (255, 255, 0) => Some(Self::Stats),
            (255, 0, 255) => Some(Self::TypeLine),
            _ => None,
        }
    }
}

// ============================================================================
// Card template — parsed from mask, cached in state
// ============================================================================

/// Normalised region rect (0..1 UV space within the template image).
#[derive(Clone, Copy, Default, Debug)]
struct NormRect {
    u_min: f32,
    v_min: f32,
    u_max: f32,
    v_max: f32,
}

impl NormRect {
    /// Scale this normalised rect into a concrete pixel rect within `card_rect`.
    fn to_rect(self, card_rect: Rect) -> Rect {
        let x0 = card_rect.left() + self.u_min * card_rect.width();
        let y0 = card_rect.top() + self.v_min * card_rect.height();
        let x1 = card_rect.left() + self.u_max * card_rect.width();
        let y1 = card_rect.top() + self.v_max * card_rect.height();
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }
}

/// Parsed card template — regions extracted from mask, frame texture loaded separately.
#[derive(Clone, Default, Debug)]
struct CardTemplate {
    title: Option<NormRect>,
    art: Option<NormRect>,
    description: Option<NormRect>,
    stats: Option<NormRect>,
    type_line: Option<NormRect>,
}

impl CardTemplate {
    /// Parse a colour-keyed mask image into normalised region rects.
    fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        // Track bounding boxes per region
        struct Bounds {
            min_x: u32,
            min_y: u32,
            max_x: u32,
            max_y: u32,
        }

        let mut regions: std::collections::HashMap<CardRegion, Bounds> =
            std::collections::HashMap::new();

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let r = rgba[idx];
                let g = rgba[idx + 1];
                let b = rgba[idx + 2];

                if let Some(region) = CardRegion::from_rgb(r, g, b) {
                    let bounds = regions.entry(region).or_insert(Bounds {
                        min_x: x,
                        min_y: y,
                        max_x: x,
                        max_y: y,
                    });
                    bounds.min_x = bounds.min_x.min(x);
                    bounds.min_y = bounds.min_y.min(y);
                    bounds.max_x = bounds.max_x.max(x);
                    bounds.max_y = bounds.max_y.max(y);
                }
            }
        }

        let to_norm = |b: &Bounds| NormRect {
            u_min: b.min_x as f32 / width as f32,
            v_min: b.min_y as f32 / height as f32,
            u_max: (b.max_x + 1) as f32 / width as f32,
            v_max: (b.max_y + 1) as f32 / height as f32,
        };

        Self {
            title: regions.get(&CardRegion::Title).map(to_norm),
            art: regions.get(&CardRegion::Art).map(to_norm),
            description: regions.get(&CardRegion::Description).map(to_norm),
            stats: regions.get(&CardRegion::Stats).map(to_norm),
            type_line: regions.get(&CardRegion::TypeLine).map(to_norm),
        }
    }
}

// ============================================================================
// State
// ============================================================================

pub struct TcgCardState {
    // Demo 1: Card Frame
    pub card_width: f32,
    pub card_height: f32,
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

    // Template state (cached after first parse)
    template: Option<CardTemplate>,
    mask_load_attempted: bool,
}

impl Default for TcgCardState {
    fn default() -> Self {
        Self {
            card_width: 250.0,
            card_height: 350.0,
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

            template: None,
            mask_load_attempted: false,
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

// ============================================================================
// Utility functions
// ============================================================================

fn darken(c: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        c.r().saturating_sub(amount),
        c.g().saturating_sub(amount),
        c.b().saturating_sub(amount),
        c.a(),
    )
}

fn bilinear(corners: [Pos2; 4], u: f32, v: f32) -> Pos2 {
    let [tl, tr, br, bl] = corners;
    let top_x = tl.x + (tr.x - tl.x) * u;
    let top_y = tl.y + (tr.y - tl.y) * u;
    let bot_x = bl.x + (br.x - bl.x) * u;
    let bot_y = bl.y + (br.y - bl.y) * u;
    Pos2::new(top_x + (bot_x - top_x) * v, top_y + (bot_y - top_y) * v)
}

fn stroke_quad(painter: &egui::Painter, corners: &[Pos2; 4], stroke: egui::Stroke) {
    for i in 0..4 {
        painter.line_segment([corners[i], corners[(i + 1) % 4]], stroke);
    }
}

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

/// Compute UV bounds for a 1:1 cover crop within a region of given dimensions.
fn cover_crop_uvs(region_w: f32, region_h: f32) -> (Pos2, Pos2) {
    let region_aspect = region_w / region_h;
    if region_aspect >= 1.0 {
        let v_span = 1.0 / region_aspect;
        let v_offset = (1.0 - v_span) / 2.0;
        (Pos2::new(0.0, v_offset), Pos2::new(1.0, v_offset + v_span))
    } else {
        let u_span = region_aspect;
        let u_offset = (1.0 - u_span) / 2.0;
        (Pos2::new(u_offset, 0.0), Pos2::new(u_offset + u_span, 1.0))
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
    uv_min: Pos2,
    uv_max: Pos2,
) {
    let uvs = [
        Pos2::new(uv_min.x, uv_min.y),
        Pos2::new(uv_max.x, uv_min.y),
        Pos2::new(uv_max.x, uv_max.y),
        Pos2::new(uv_min.x, uv_max.y),
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

fn draw_textured_quad_subdivided(
    painter: &egui::Painter,
    corners: [Pos2; 4],
    texture_id: egui::TextureId,
    tint: Color32,
    subdivisions: u32,
    uv_min: Pos2,
    uv_max: Pos2,
) {
    let cols = subdivisions;
    let rows = subdivisions;
    let mut mesh = Mesh::with_texture(texture_id);

    for row in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = row as f32 / rows as f32;
            let pos = bilinear(corners, u, v);
            let tex_u = uv_min.x + (uv_max.x - uv_min.x) * u;
            let tex_v = uv_min.y + (uv_max.y - uv_min.y) * v;
            mesh.vertices.push(Vertex {
                pos,
                uv: Pos2::new(tex_u, tex_v),
                color: tint,
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

/// Try to load and parse the mask PNG. Returns the parsed template if successful.
fn try_parse_mask(ctx: &egui::Context) -> Option<CardTemplate> {
    let bytes = match ctx.try_load_bytes(MASK_URL) {
        Ok(egui::load::BytesPoll::Ready { bytes, .. }) => bytes,
        _ => return None,
    };

    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(CardTemplate::from_rgba(w, h, rgba.as_raw()))
}

// ============================================================================
// Template-driven card rendering
// ============================================================================

/// Map a sub-rect into a perspective quad using normalised coordinates.
fn map_rect_to_quad(card_rect: Rect, corners: [Pos2; 4], sub_rect: Rect) -> [Pos2; 4] {
    let ow = card_rect.width();
    let oh = card_rect.height();
    let u0 = (sub_rect.left() - card_rect.left()) / ow;
    let u1 = (sub_rect.right() - card_rect.left()) / ow;
    let v0 = (sub_rect.top() - card_rect.top()) / oh;
    let v1 = (sub_rect.bottom() - card_rect.top()) / oh;
    [
        bilinear(corners, u0, v0),
        bilinear(corners, u1, v0),
        bilinear(corners, u1, v1),
        bilinear(corners, u0, v1),
    ]
}

/// Draw the card front (flat, no perspective).
///
/// Layer order: rarity border → art background → frame overlay → text labels
fn draw_card_front_flat(
    painter: &egui::Painter,
    card_rect: Rect,
    template: &CardTemplate,
    rarity: usize,
    art_texture: Option<egui::TextureId>,
    frame_texture: Option<egui::TextureId>,
) {
    let rarity_col = rarity_color(rarity);
    let card_bg = Color32::from_rgb(30, 30, 48);

    // 1. Rarity border (thin outline)
    painter.rect_stroke(
        card_rect,
        4.0,
        egui::Stroke::new(3.0, rarity_col),
        egui::StrokeKind::Outside,
    );

    // 2. Card background (fallback if no art)
    painter.rect_filled(card_rect, 4.0, card_bg);

    // 3. Art image — into the art region from template, or full card
    if let Some(tex_id) = art_texture {
        let art_rect = template
            .art
            .map(|nr| nr.to_rect(card_rect))
            .unwrap_or(card_rect);
        let (uv_min, uv_max) = cover_crop_uvs(art_rect.width(), art_rect.height());
        let art_corners = [
            art_rect.left_top(),
            art_rect.right_top(),
            art_rect.right_bottom(),
            art_rect.left_bottom(),
        ];
        draw_textured_quad(painter, art_corners, tex_id, Color32::WHITE, uv_min, uv_max);
    }

    // 4. Frame overlay — full card, alpha-blended on top
    if let Some(frame_tex) = frame_texture {
        let frame_corners = [
            card_rect.left_top(),
            card_rect.right_top(),
            card_rect.right_bottom(),
            card_rect.left_bottom(),
        ];
        let uv_full = (Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        draw_textured_quad(
            painter,
            frame_corners,
            frame_tex,
            Color32::WHITE,
            uv_full.0,
            uv_full.1,
        );
    }

    // 5. Text labels into template regions
    draw_card_labels(painter, card_rect, template, rarity);
}

/// Draw the card front with perspective tilt.
///
/// Same layer order as flat, but everything bilinearly mapped into the tilted quad.
#[allow(clippy::too_many_arguments)]
fn draw_card_front_perspective(
    ui: &egui::Ui,
    painter: &egui::Painter,
    card_rect: Rect,
    corners: [Pos2; 4],
    template: &CardTemplate,
    rarity: usize,
    art_texture: Option<egui::TextureId>,
    frame_texture: Option<egui::TextureId>,
) {
    let rarity_col = rarity_color(rarity);
    let card_bg = Color32::from_rgb(30, 30, 48);

    // 1. Rarity border
    stroke_quad(painter, &corners, egui::Stroke::new(3.0, rarity_col));

    // 2. Card background
    draw_quad(painter, corners, card_bg);

    // 3. Art image
    if let Some(tex_id) = art_texture {
        let art_rect = template
            .art
            .map(|nr| nr.to_rect(card_rect))
            .unwrap_or(card_rect);
        let art_corners = map_rect_to_quad(card_rect, corners, art_rect);
        let (uv_min, uv_max) = cover_crop_uvs(art_rect.width(), art_rect.height());
        draw_textured_quad_subdivided(
            painter,
            art_corners,
            tex_id,
            Color32::WHITE,
            8,
            uv_min,
            uv_max,
        );
    }

    // 4. Frame overlay
    if let Some(frame_tex) = frame_texture {
        let full_uv = (Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        draw_textured_quad_subdivided(
            painter,
            corners,
            frame_tex,
            Color32::WHITE,
            8,
            full_uv.0,
            full_uv.1,
        );
    }

    // 5. Text labels (perspective-mapped)
    draw_card_labels_perspective(ui, painter, card_rect, corners, template, rarity);
}

/// Draw text labels into template-defined regions (flat).
fn draw_card_labels(
    painter: &egui::Painter,
    card_rect: Rect,
    template: &CardTemplate,
    rarity: usize,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let muted_color = Color32::from_rgb(140, 140, 160);

    // Title
    if let Some(nr) = template.title {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout_no_wrap(
            "Shadow Drake".to_string(),
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
            text_color,
        );
        let pos = Pos2::new(r.left() + 8.0, r.center().y - galley.size().y / 2.0);
        let clipped = painter.clip_rect().intersect(r);
        if clipped.is_positive() {
            painter
                .with_clip_rect(clipped)
                .galley(pos, galley, Color32::TRANSPARENT);
        }
    }

    // Type line
    if let Some(nr) = template.type_line {
        let r = nr.to_rect(card_rect);
        let type_text = RARITIES.get(rarity).map_or("Common", |t| t.0);
        let galley = painter.layout_no_wrap(
            format!("Dragon  —  {type_text}"),
            egui::FontId::new(9.0, egui::FontFamily::Monospace),
            muted_color,
        );
        let pos = Pos2::new(r.left() + 8.0, r.center().y - galley.size().y / 2.0);
        let clipped = painter.clip_rect().intersect(r);
        if clipped.is_positive() {
            painter
                .with_clip_rect(clipped)
                .galley(pos, galley, Color32::TRANSPARENT);
        }
    }

    // Description
    if let Some(nr) = template.description {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout(
            "When this creature enters\nthe battlefield, deal 3\ndamage to target player."
                .to_string(),
            egui::FontId::new(10.0, egui::FontFamily::Monospace),
            muted_color,
            r.width() - 16.0,
        );
        let pos = Pos2::new(r.left() + 8.0, r.top() + 6.0);
        let clipped = painter.clip_rect().intersect(r);
        if clipped.is_positive() {
            painter
                .with_clip_rect(clipped)
                .galley(pos, galley, Color32::TRANSPARENT);
        }
    }

    // Stats
    if let Some(nr) = template.stats {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout_no_wrap(
            "ATK 5  /  DEF 3".to_string(),
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
            text_color,
        );
        let pos = Pos2::new(
            r.center().x - galley.size().x / 2.0,
            r.center().y - galley.size().y / 2.0,
        );
        let clipped = painter.clip_rect().intersect(r);
        if clipped.is_positive() {
            painter
                .with_clip_rect(clipped)
                .galley(pos, galley, Color32::TRANSPARENT);
        }
    }
}

/// Draw text labels mapped into perspective quad.
fn draw_card_labels_perspective(
    ui: &egui::Ui,
    painter: &egui::Painter,
    card_rect: Rect,
    corners: [Pos2; 4],
    template: &CardTemplate,
    rarity: usize,
) {
    let text_color = Color32::from_rgb(220, 220, 235);
    let muted_color = Color32::from_rgb(140, 140, 160);

    let font_tex_size = ui.ctx().fonts(|f| f.font_image_size());
    let uv_norm = Vec2::new(1.0 / font_tex_size[0] as f32, 1.0 / font_tex_size[1] as f32);

    let draw_mapped = |galley: &std::sync::Arc<egui::Galley>, text_pos: Pos2| {
        let mut text_mesh = Mesh::with_texture(egui::TextureId::default());
        for placed_row in &galley.rows {
            let row_offset = placed_row.pos;
            let row_mesh = &placed_row.row.visuals.mesh;
            let idx_offset = text_mesh.vertices.len() as u32;
            for vertex in &row_mesh.vertices {
                let abs_x = text_pos.x + row_offset.x + vertex.pos.x;
                let abs_y = text_pos.y + row_offset.y + vertex.pos.y;
                let u = (abs_x - card_rect.left()) / card_rect.width();
                let v = (abs_y - card_rect.top()) / card_rect.height();
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
    if let Some(nr) = template.title {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout_no_wrap(
            "Shadow Drake".to_string(),
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
            text_color,
        );
        let pos = Pos2::new(r.left() + 8.0, r.center().y - galley.size().y / 2.0);
        draw_mapped(&galley, pos);
    }

    // Type line
    if let Some(nr) = template.type_line {
        let r = nr.to_rect(card_rect);
        let type_text = RARITIES.get(rarity).map_or("Common", |t| t.0);
        let galley = painter.layout_no_wrap(
            format!("Dragon  —  {type_text}"),
            egui::FontId::new(9.0, egui::FontFamily::Monospace),
            muted_color,
        );
        let pos = Pos2::new(r.left() + 8.0, r.center().y - galley.size().y / 2.0);
        draw_mapped(&galley, pos);
    }

    // Description
    if let Some(nr) = template.description {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout(
            "When this creature enters\nthe battlefield, deal 3\ndamage to target player."
                .to_string(),
            egui::FontId::new(10.0, egui::FontFamily::Monospace),
            muted_color,
            r.width() - 16.0,
        );
        let pos = Pos2::new(r.left() + 8.0, r.top() + 6.0);
        draw_mapped(&galley, pos);
    }

    // Stats
    if let Some(nr) = template.stats {
        let r = nr.to_rect(card_rect);
        let galley = painter.layout_no_wrap(
            "ATK 5  /  DEF 3".to_string(),
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
            text_color,
        );
        let pos = Pos2::new(
            r.center().x - galley.size().x / 2.0,
            r.center().y - galley.size().y / 2.0,
        );
        draw_mapped(&galley, pos);
    }
}

// ============================================================================
// Tilt
// ============================================================================

fn tilt_corners(rect: Rect, tilt_x: f32, tilt_y: f32, pinch_factor: f32) -> [Pos2; 4] {
    let w = rect.width();
    let h = rect.height();
    let px = w * pinch_factor * tilt_x;
    let py = h * pinch_factor * tilt_y;
    [
        Pos2::new(rect.left() + px.max(0.0), rect.top() + py.max(0.0)),
        Pos2::new(rect.right() + px.min(0.0), rect.top() - py.min(0.0)),
        Pos2::new(rect.right() - px.max(0.0), rect.bottom() - py.max(0.0)),
        Pos2::new(rect.left() - px.min(0.0), rect.bottom() + py.min(0.0)),
    ]
}

// ============================================================================
// Holographic overlay
// ============================================================================

#[allow(clippy::too_many_arguments)]
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
    let cols = 12_u32;
    let rows = 18_u32;

    let light_x = mouse_u - 0.5;
    let light_y = mouse_v - 0.5;
    let light_len = (light_x * light_x + light_y * light_y).sqrt().max(0.001);
    let light_dx = light_x / light_len;
    let light_dy = light_y / light_len;

    let mut mesh = Mesh::default();

    for row in 0..=rows {
        for col in 0..=cols {
            let u = col as f32 / cols as f32;
            let v = row as f32 / rows as f32;
            let pos = bilinear(corners, u, v);

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
            let fresnel = 1.0 - (edge_u.min(edge_v)).clamp(0.0, 1.0);
            let fresnel_boost = fresnel * fresnel * overlay_opacity * 0.5;
            let intensity = (streak + fresnel_boost).clamp(0.0, 1.0);

            let rainbow = hue_to_rgb(hue);
            let alpha = (intensity * 255.0) as u8;
            let color = Color32::from_rgba_premultiplied(
                (rainbow.r() as f32 * intensity) as u8,
                (rainbow.g() as f32 * intensity) as u8,
                (rainbow.b() as f32 * intensity) as u8,
                alpha,
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

// ============================================================================
// Card back
// ============================================================================

fn draw_card_back(painter: &egui::Painter, corners: [Pos2; 4], rarity: usize) {
    let rarity_col = rarity_color(rarity);
    let bg = Color32::from_rgb(20, 20, 38);

    draw_quad(painter, corners, bg);

    let grid_cols = 6_u32;
    let grid_rows = 8_u32;
    let diamond_color = darken(rarity_col, 120);

    for row in 0..grid_rows {
        for col in 0..grid_cols {
            let cu = (col as f32 + 0.5) / grid_cols as f32;
            let cv = (row as f32 + 0.5) / grid_rows as f32;
            let du = 0.3 / grid_cols as f32;
            let dv = 0.3 / grid_rows as f32;
            let diamond = [
                bilinear(corners, cu, cv - dv),
                bilinear(corners, cu + du, cv),
                bilinear(corners, cu, cv + dv),
                bilinear(corners, cu - du, cv),
            ];
            draw_quad(painter, diamond, diamond_color);
        }
    }

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

// ============================================================================
// Main show function
// ============================================================================

pub fn show(ui: &mut egui::Ui, state: &mut TcgCardState) {
    let dt = ui.input(|i| i.stable_dt).min(0.1);

    // Load textures (cached by egui after first load)
    let art_texture = try_load_texture(ui.ctx(), IIIF_ART_URL);
    let frame_texture = try_load_texture(ui.ctx(), FRAME_URL);

    // Parse mask template (once)
    if state.template.is_none() && !state.mask_load_attempted {
        if let Some(tmpl) = try_parse_mask(ui.ctx()) {
            log::info!(
                "Parsed card template: title={} art={} desc={} stats={} type={}",
                tmpl.title.is_some(),
                tmpl.art.is_some(),
                tmpl.description.is_some(),
                tmpl.stats.is_some(),
                tmpl.type_line.is_some(),
            );
            state.template = Some(tmpl);
        }
        // If bytes aren't ready yet, try_parse_mask returns None — we'll try again next frame.
        // Only mark attempted once we actually got a result (success or decode failure).
        if state.template.is_some() {
            state.mask_load_attempted = true;
        }
    }

    // Fallback template if mask hasn't loaded yet — hardcoded approximation
    let fallback = CardTemplate {
        title: Some(NormRect {
            u_min: 0.032,
            v_min: 0.023,
            u_max: 0.968,
            v_max: 0.086,
        }),
        art: Some(NormRect {
            u_min: 0.032,
            v_min: 0.086,
            u_max: 0.968,
            v_max: 0.580,
        }),
        type_line: Some(NormRect {
            u_min: 0.032,
            v_min: 0.580,
            u_max: 0.968,
            v_max: 0.620,
        }),
        description: Some(NormRect {
            u_min: 0.032,
            v_min: 0.620,
            u_max: 0.968,
            v_max: 0.903,
        }),
        stats: Some(NormRect {
            u_min: 0.032,
            v_min: 0.903,
            u_max: 0.968,
            v_max: 0.977,
        }),
    };
    let template = state.template.as_ref().unwrap_or(&fallback);

    // --- 1. Card Frame ---
    ui.label(egui::RichText::new("1. Card Frame").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Template-driven layout: art background, frame overlay, text in mask-defined regions.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(&mut state.card_width, 140.0..=400.0).text("Width"));
        ui.add(egui::Slider::new(&mut state.card_height, 200.0..=560.0).text("Height"));
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

    // Template status
    let tmpl_status = if state.template.is_some() {
        "loaded"
    } else {
        "loading..."
    };
    let frame_status = if frame_texture.is_some() {
        "loaded"
    } else {
        "loading..."
    };
    let art_status = if art_texture.is_some() {
        "loaded"
    } else {
        "loading..."
    };
    ui.label(
        egui::RichText::new(format!(
            "Template: {tmpl_status}  |  Frame: {frame_status}  |  Art: {art_status}"
        ))
        .color(TEXT_MUTED)
        .small(),
    );
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

    draw_card_front_flat(
        &painter,
        card_rect,
        template,
        state.rarity,
        art_texture,
        frame_texture,
    );

    ui.add_space(16.0);

    // --- 2. Perspective Tilt ---
    ui.label(
        egui::RichText::new("2. Perspective Tilt (Mouse-Driven)")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("Card tilts toward the mouse cursor. All layers bilinearly mapped.")
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

    let (target_x, target_y) = if let Some(hover_pos) = response2.hover_pos() {
        let rel_x = (hover_pos.x - card_rect2.center().x) / (card_rect2.width() / 2.0);
        let rel_y = (hover_pos.y - card_rect2.center().y) / (card_rect2.height() / 2.0);
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };

    state.current_tilt_x += (target_x - state.current_tilt_x) * state.tilt_ease;
    state.current_tilt_y += (target_y - state.current_tilt_y) * state.tilt_ease;

    if state.current_tilt_x.abs() > 0.001 || state.current_tilt_y.abs() > 0.001 {
        ui.ctx().request_repaint();
    }

    let corners2 = tilt_corners(
        card_rect2,
        state.current_tilt_x,
        state.current_tilt_y,
        state.pinch_factor,
    );

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

    draw_card_front_perspective(
        ui,
        &painter2,
        card_rect2,
        corners2,
        template,
        state.rarity,
        art_texture,
        frame_texture,
    );

    ui.add_space(16.0);

    // --- 3. Holographic / Foil Effect ---
    ui.label(
        egui::RichText::new("3. Holographic / Foil Effect")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Specular streak, iridescence, and fresnel edge glow. Hover to see the effect.",
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
    let corners3 = [
        card_rect3.left_top(),
        card_rect3.right_top(),
        card_rect3.right_bottom(),
        card_rect3.left_bottom(),
    ];

    draw_card_front_flat(
        &painter3,
        card_rect3,
        template,
        state.rarity,
        art_texture,
        frame_texture,
    );

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
        egui::RichText::new("Physics-based 180° flip with vertical lift and edge thickness.")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

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

    let lift_headroom = 20.0;
    let (rect4, response4) = ui.allocate_exact_size(
        Vec2::new(
            state.card_width + 40.0,
            state.card_height + 20.0 + lift_headroom,
        ),
        egui::Sense::click(),
    );
    let painter4 = ui.painter_at(rect4);

    if response4.clicked() && !state.flip_animating {
        state.flip_animating = true;
        state.flip_progress = 0.0;
    }

    let p = state.flip_progress;
    let angle = p * std::f32::consts::PI;
    let cos_a = angle.cos();
    let width_fraction = cos_a.abs();
    let showing_front = if state.showing_back {
        cos_a < 0.0
    } else {
        cos_a >= 0.0
    };
    let lift = angle.sin() * lift_headroom;

    let base_center = Pos2::new(rect4.center().x, rect4.center().y + lift_headroom / 2.0);
    let lifted_center = Pos2::new(base_center.x, base_center.y - lift);
    let card_rect4 = Rect::from_center_size(
        lifted_center,
        Vec2::new(state.card_width, state.card_height),
    );

    // Shadow
    let shadow_alpha = (0.25 * (1.0 + lift / lift_headroom)) * 80.0;
    let shadow_spread = lift * 0.3;
    let shadow_rect = Rect::from_center_size(
        Pos2::new(base_center.x, base_center.y + 4.0 + shadow_spread * 0.5),
        Vec2::new(
            state.card_width * width_fraction + shadow_spread * 2.0,
            state.card_height + shadow_spread,
        ),
    );
    painter4.rect_filled(
        shadow_rect,
        8.0,
        Color32::from_rgba_premultiplied(0, 0, 0, shadow_alpha as u8),
    );

    let cx = card_rect4.center().x;
    let half_w = state.card_width / 2.0 * width_fraction;
    let edge_thickness = 2.0;
    let edge_visible = (1.0 - width_fraction) * edge_thickness;

    if width_fraction > 0.01 {
        let flip_corners = [
            Pos2::new(cx - half_w, card_rect4.top()),
            Pos2::new(cx + half_w, card_rect4.top()),
            Pos2::new(cx + half_w, card_rect4.bottom()),
            Pos2::new(cx - half_w, card_rect4.bottom()),
        ];

        if showing_front {
            draw_card_front_perspective(
                ui,
                &painter4,
                card_rect4,
                flip_corners,
                template,
                state.rarity,
                art_texture,
                frame_texture,
            );
        } else {
            draw_card_back(&painter4, flip_corners, state.rarity);
        }

        stroke_quad(
            &painter4,
            &flip_corners,
            egui::Stroke::new(2.0, rarity_color(state.rarity)),
        );
    }

    if edge_visible > 0.5 {
        let edge_color = Color32::from_rgb(60, 60, 80);
        let edge_corners = [
            Pos2::new(cx + half_w, card_rect4.top()),
            Pos2::new(cx + half_w + edge_visible, card_rect4.top()),
            Pos2::new(cx + half_w + edge_visible, card_rect4.bottom()),
            Pos2::new(cx + half_w, card_rect4.bottom()),
        ];
        draw_quad(&painter4, edge_corners, edge_color);
    }

    let face_label = if showing_front { "Front" } else { "Back" };
    let face_state = if state.showing_back { "Back" } else { "Front" };
    ui.label(
        egui::RichText::new(format!(
            "Showing: {face_label} (base: {face_state}, lift: {lift:.1}px, width: {:.0}%)",
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
            "All effects combined: perspective tilt, holographic overlay (Rare+), click to flip.",
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

    let (target5_x, target5_y) = if let Some(hover_pos) = response5.hover_pos() {
        let rel_x = (hover_pos.x - card_rect5.center().x) / (card_rect5.width() / 2.0);
        let rel_y = (hover_pos.y - card_rect5.center().y) / (card_rect5.height() / 2.0);
        (rel_x.clamp(-1.0, 1.0), rel_y.clamp(-1.0, 1.0))
    } else {
        (0.0, 0.0)
    };

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

    if state.showing_back && !state.flip_animating {
        draw_card_back(&painter5, corners5, state.rarity);
        stroke_quad(
            &painter5,
            &corners5,
            egui::Stroke::new(2.0, rarity_color(state.rarity)),
        );
    } else {
        draw_card_front_perspective(
            ui,
            &painter5,
            card_rect5,
            corners5,
            template,
            state.rarity,
            art_texture,
            frame_texture,
        );

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

    if response5.clicked() {
        state.showing_back = !state.showing_back;
    }

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Key patterns:").color(ACCENT).strong());
    ui.label("- Template-driven: frame.png overlay + mask.png colour-keyed regions");
    ui.label("- Art: cover-cropped 1:1 into template art region, behind frame");
    ui.label("- Perspective: all layers bilinearly mapped into tilted quad");
    ui.label("- Text: galley mesh vertices mapped into perspective space");
    ui.label("- Holographic: specular streak + iridescence + fresnel edge glow");
    ui.label("- Flip: cos-based rotation with vertical lift and edge thickness");
}
