//! Mesh Playground story — demonstrates the egui `Mesh` API for custom rendering.
//!
//! Shows: solid-colour quads, vertex colour gradients, trapezoids,
//! animated rotation, and interactive vertex manipulation.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Vec2};

use crate::{ACCENT, TEXT_MUTED};

pub struct MeshPlaygroundState {
    pub rotation: f32,
    pub animating: bool,
    pub pinch: f32,
    pub wave_phase: f32,
}

impl Default for MeshPlaygroundState {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            animating: false,
            pinch: 0.0,
            wave_phase: 0.0,
        }
    }
}

const WHITE_UV: Pos2 = Pos2::new(0.0, 0.0);

pub fn show(ui: &mut egui::Ui, state: &mut MeshPlaygroundState) {
    let dt = ui.input(|i| i.stable_dt).min(0.1);

    if state.animating {
        state.rotation += dt * 1.5;
        state.wave_phase += dt * 3.0;
        ui.ctx().request_repaint();
    }

    // --- 1. Solid colour quad ---
    ui.label(
        egui::RichText::new("1. Solid Colour Quad")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new("4 vertices, 2 triangles, single colour. The simplest mesh.")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 60.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    draw_solid_quad(&painter, rect, Color32::from_rgb(68, 120, 255));

    ui.add_space(16.0);

    // --- 2. Vertex colour gradient ---
    ui.label(
        egui::RichText::new("2. Vertex Colour Gradient")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Each corner has a different colour. GPU interpolates between vertices.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(Vec2::new(200.0, 80.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    draw_gradient_quad(
        &painter,
        rect,
        Color32::from_rgb(255, 60, 60),  // top-left: red
        Color32::from_rgb(60, 255, 60),  // top-right: green
        Color32::from_rgb(60, 60, 255),  // bottom-right: blue
        Color32::from_rgb(255, 255, 60), // bottom-left: yellow
    );

    ui.add_space(16.0);

    // --- 3. Interactive trapezoid ---
    ui.label(
        egui::RichText::new("3. Interactive Trapezoid")
            .color(ACCENT)
            .strong(),
    );
    ui.label(egui::RichText::new("Drag the slider to pinch the top edge inward. This is how the flip counter sells perspective.").color(TEXT_MUTED).small());
    ui.add_space(4.0);

    ui.add(egui::Slider::new(&mut state.pinch, 0.0..=0.45).text("Pinch"));
    ui.add_space(4.0);

    let (rect, _) = ui.allocate_exact_size(Vec2::new(160.0, 80.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let pinch_px = rect.width() * state.pinch;
    draw_trapezoid(
        &painter,
        rect.left() + pinch_px,
        rect.top(),
        rect.right() - pinch_px,
        rect.top(),
        rect.right(),
        rect.bottom(),
        rect.left(),
        rect.bottom(),
        Color32::from_rgb(45, 45, 65),
    );

    ui.add_space(16.0);

    // --- 4. Rotating quad ---
    ui.label(
        egui::RichText::new("4. Rotating Quad")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Vertices computed with sin/cos rotation matrix. No GPU transform — pure vertex math.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui
            .button(if state.animating { "Pause" } else { "Animate" })
            .clicked()
        {
            state.animating = !state.animating;
        }
        if ui.button("Reset").clicked() {
            state.rotation = 0.0;
            state.wave_phase = 0.0;
        }
    });
    ui.add_space(4.0);

    let size = 80.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size + 20.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let half = size / 2.0;
    let cos = state.rotation.cos();
    let sin = state.rotation.sin();

    // Rotate 4 corner offsets around center
    let corners = [(-half, -half), (half, -half), (half, half), (-half, half)];
    let rotated: Vec<Pos2> = corners
        .iter()
        .map(|(x, y)| Pos2::new(center.x + x * cos - y * sin, center.y + x * sin + y * cos))
        .collect();

    let mut mesh = Mesh::default();
    let colors = [
        Color32::from_rgb(255, 100, 100),
        Color32::from_rgb(100, 255, 100),
        Color32::from_rgb(100, 100, 255),
        Color32::from_rgb(255, 255, 100),
    ];
    for (pos, color) in rotated.iter().zip(colors.iter()) {
        mesh.vertices.push(Vertex {
            pos: *pos,
            uv: WHITE_UV,
            color: *color,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));

    ui.add_space(16.0);

    // --- 5. Wave mesh ---
    ui.label(egui::RichText::new("5. Wave Mesh").color(ACCENT).strong());
    ui.label(egui::RichText::new("Multi-segment mesh with sinusoidal vertex displacement. Shows how to build strip geometry.").color(TEXT_MUTED).small());
    ui.add_space(4.0);

    let wave_w = 300.0_f32;
    let wave_h = 60.0_f32;
    let segments = 30_u32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(wave_w, wave_h + 20.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let mut mesh = Mesh::default();
    let seg_w = wave_w / segments as f32;

    for i in 0..=segments {
        let x = rect.left() + i as f32 * seg_w;
        let t = i as f32 / segments as f32;
        let wave_y = (t * std::f32::consts::TAU * 2.0 + state.wave_phase).sin() * 10.0;

        // Top vertex
        mesh.vertices.push(Vertex {
            pos: Pos2::new(x, rect.top() + 10.0 + wave_y),
            uv: WHITE_UV,
            color: Color32::from_rgb((68.0 + t * 187.0) as u8, (255.0 - t * 200.0) as u8, 255),
        });
        // Bottom vertex
        mesh.vertices.push(Vertex {
            pos: Pos2::new(x, rect.bottom() - 10.0 + wave_y * 0.3),
            uv: WHITE_UV,
            color: Color32::from_rgb((40.0 + t * 100.0) as u8, 40, (180.0 - t * 80.0) as u8),
        });

        // Add two triangles for each quad (except the first column)
        if i > 0 {
            let base = (i - 1) * 2;
            // Triangle 1: prev-top, curr-top, prev-bottom
            mesh.indices.extend_from_slice(&[base, base + 2, base + 1]);
            // Triangle 2: prev-bottom, curr-top, curr-bottom
            mesh.indices
                .extend_from_slice(&[base + 1, base + 2, base + 3]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));

    ui.add_space(16.0);

    // --- 6. Diamond / polygon ---
    ui.label(egui::RichText::new("6. N-gon Fan").color(ACCENT).strong());
    ui.label(
        egui::RichText::new(
            "Triangle fan from center point. Any convex polygon can be built this way.",
        )
        .color(TEXT_MUTED)
        .small(),
    );
    ui.add_space(4.0);

    let ngon_size = 70.0_f32;
    let (rect, _) =
        ui.allocate_exact_size(Vec2::splat(ngon_size * 2.0 + 10.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let sides = 6;
    let mut mesh = Mesh::default();

    // Center vertex
    mesh.vertices.push(Vertex {
        pos: center,
        uv: WHITE_UV,
        color: Color32::from_rgb(255, 255, 255),
    });

    for i in 0..=sides {
        let angle = (i as f32 / sides as f32) * std::f32::consts::TAU + state.rotation * 0.5;
        let px = center.x + angle.cos() * ngon_size;
        let py = center.y + angle.sin() * ngon_size;
        let hue = (i as f32 / sides as f32 * 360.0) as u16;
        let color = hue_to_rgb(hue);

        mesh.vertices.push(Vertex {
            pos: Pos2::new(px, py),
            uv: WHITE_UV,
            color,
        });

        if i > 0 {
            mesh.indices.extend_from_slice(&[0, i as u32, i as u32 + 1]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Key patterns:").color(ACCENT).strong());
    ui.label("- Mesh::default() uses TextureId::Managed(0) with WHITE_UV at (0,0)");
    ui.label("- Vertex colours are interpolated by the GPU across triangles");
    ui.label("- Rotation/transforms are done in vertex math, not GPU transforms");
    ui.label("- Triangle fans build convex polygons from a center point");
    ui.label("- Strip geometry (wave) uses alternating top/bottom vertices");
}

fn draw_solid_quad(painter: &egui::Painter, rect: Rect, color: Color32) {
    let mut mesh = Mesh::default();
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: rect.left_top(),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: rect.right_top(),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: rect.right_bottom(),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: rect.left_bottom(),
            uv: WHITE_UV,
            color,
        },
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

fn draw_gradient_quad(
    painter: &egui::Painter,
    rect: Rect,
    tl: Color32,
    tr: Color32,
    br: Color32,
    bl: Color32,
) {
    let mut mesh = Mesh::default();
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: rect.left_top(),
            uv: WHITE_UV,
            color: tl,
        },
        Vertex {
            pos: rect.right_top(),
            uv: WHITE_UV,
            color: tr,
        },
        Vertex {
            pos: rect.right_bottom(),
            uv: WHITE_UV,
            color: br,
        },
        Vertex {
            pos: rect.left_bottom(),
            uv: WHITE_UV,
            color: bl,
        },
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
fn draw_trapezoid(
    painter: &egui::Painter,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    color: Color32,
) {
    let mut mesh = Mesh::default();
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: Pos2::new(x0, y0),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: Pos2::new(x1, y1),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: Pos2::new(x2, y2),
            uv: WHITE_UV,
            color,
        },
        Vertex {
            pos: Pos2::new(x3, y3),
            uv: WHITE_UV,
            color,
        },
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// Simple HSV→RGB for vertex colouring (saturation=1, value=1).
fn hue_to_rgb(hue: u16) -> Color32 {
    let h = (hue % 360) as f32 / 60.0;
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
