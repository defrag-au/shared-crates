//! `TransformStick` story — a hand, and a subject somewhere else.
//!
//! The two are side by side on purpose, and the right-hand one is deliberately
//! distorted: that is the mug's arrangement, and the reason this control exists.
//! A decal's screen shape bends with the surface it is going on, so a control
//! drawn on that shape can only approximate the thing it is showing. A stick does
//! not try: it is a small fixed thing that is pushed, and the subject moves where
//! it actually lives.
//!
//! Push it — hold the push, and note that it keeps going. Nothing here is limited
//! by how far a drag could reach, which is the whole argument for a rate over a
//! position.

use egui_widgets::placement::Placement;
use egui_widgets::theme::{Space, ThemeExt};
use egui_widgets::transform_stick::{Rates, TransformStick};

use crate::{caption, heading};

/// The subject's space, in the caller's own units — a decal's unit square.
fn surface() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// The preview's size on screen.
const PREVIEW: egui::Vec2 = egui::vec2(360.0, 200.0);

pub struct TransformStickState {
    stick: TransformStick,
    placement: Placement,
    texture: Option<egui::TextureHandle>,
}

impl Default for TransformStickState {
    fn default() -> Self {
        Self {
            stick: TransformStick::default(),
            placement: Placement {
                center: egui::pos2(0.5, 0.5),
                size: egui::vec2(0.3, 0.54),
                angle: 0.0,
            },
            texture: None,
        }
    }
}

/// The same stand-in the pad story uses: a ring, a cross and a filled corner, so
/// a turn and a size read at a glance.
fn subject_image(accent: egui::Color32, edge: egui::Color32) -> egui::ColorImage {
    const SIDE: usize = 128;
    let mut image = egui::ColorImage::filled([SIDE, SIDE], egui::Color32::TRANSPARENT);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let fx = x as f32 / SIDE as f32 - 0.5;
            let fy = y as f32 / SIDE as f32 - 0.5;
            let radius = (fx * fx + fy * fy).sqrt();
            let ring = (0.30..0.44).contains(&radius);
            let cross = fx.abs() < 0.02 || fy.abs() < 0.02;
            let corner = fx < -0.28 && fy < -0.28;
            image[(x, y)] = match (ring, cross, corner) {
                (_, _, true) => edge,
                (true, _, _) => accent,
                (_, true, _) => accent.gamma_multiply(0.5),
                _ => egui::Color32::TRANSPARENT,
            };
        }
    }
    image
}

/// The subject's space as a body turned away from the reader: the same two lines
/// a mug's front is, so a rectangle placed on it is drawn bowed.
fn projected(rect: egui::Rect, point: egui::Pos2) -> egui::Pos2 {
    let theta = (point.x - 0.5) * std::f32::consts::PI;
    egui::pos2(
        rect.center().x + rect.width() * 0.5 * theta.sin(),
        rect.min.y + point.y * rect.height() + rect.height() * 0.10 * (1.0 - theta.cos()),
    )
}

pub fn show(ui: &mut egui::Ui, state: &mut TransformStickState) {
    let tokens = ui.tokens();
    let texture = state.texture.get_or_insert_with(|| {
        ui.ctx().load_texture(
            "transform-stick-subject",
            subject_image(tokens.color.text_primary, tokens.color.accent),
            egui::TextureOptions::LINEAR,
        )
    });
    let subject = egui::load::SizedTexture::new(texture.id(), texture.size_vec2());

    heading(ui, "Transform Stick");
    caption(
        ui,
        "A stick that drives a subject elsewhere: push and hold, and it keeps going. \
         Nothing about the reach of the control limits where the subject can end up.",
    );
    ui.add_space(tokens.space(Space::Sm));

    let dt = ui.ctx().input(|input| input.stable_dt).min(0.1);
    let fine = ui.ctx().input(|input| input.modifiers.shift);
    if ui.available_width() > 640.0 {
        ui.columns(2, |columns| {
            control(&mut columns[0], state, dt, fine);
            preview(&mut columns[1], &state.placement, subject);
        });
    } else {
        control(ui, state, dt, fine);
        ui.add_space(tokens.space(Space::Md));
        preview(ui, &state.placement, subject);
    }

    if state.stick.engaged() {
        // A rate needs frames even when nothing else in the surface is changing,
        // and only while the stick is actually held.
        ui.ctx().request_repaint();
    }
}

/// The control, and the numbers it is driving.
fn control(ui: &mut egui::Ui, state: &mut TransformStickState, dt: f32, fine: bool) {
    let tokens = ui.tokens();
    state.stick.mode_row(ui);
    ui.add_space(tokens.space(Space::Sm));
    state.stick.show(ui, dt);
    ui.add_space(tokens.space(Space::Sm));

    let moved = state
        .stick
        .apply(&mut state.placement, surface(), Rates::default(), dt, fine);
    if state.stick.engaged() {
        ui.weak(if fine { "held — fine" } else { "held" });
    } else if moved {
        ui.weak("released");
    }

    let u = state.placement.center.x;
    let v = state.placement.center.y;
    let turn = state.placement.angle.to_degrees();
    let width = state.placement.size.x;
    ui.weak(format!(
        "u {u:.2}   v {v:.2}   width {width:.2}   turn {turn:.0}"
    ));
    ui.weak("hold shift for a fine hand");
}

/// The subject where it actually appears — through the projection, so the reader
/// can see the thing the control is driving and not a lie about its shape.
fn preview(ui: &mut egui::Ui, placement: &Placement, subject: egui::load::SizedTexture) {
    let tokens = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(PREVIEW, egui::Sense::hover());
    let painter = ui.painter().clone();
    let corners = placement.corners();

    // The surface itself: its top and bottom edges, bowed.
    for v in [0.0, 1.0] {
        painter.add(egui::Shape::line(
            (0..=24)
                .map(|step| projected(rect, egui::pos2(step as f32 / 24.0, v)))
                .collect(),
            egui::Stroke::new(1.0_f32, tokens.color.border),
        ));
    }

    // The subject, its own rectangle carried through the same projection — which
    // is exactly what the surface does to it, and exactly what the control up
    // there never has to know about.
    let mapped = corners.map(|corner| projected(rect, corner));
    let mut mesh = egui::Mesh::with_texture(subject.id);
    let uv = [
        egui::pos2(0.0, 0.0),
        egui::pos2(1.0, 0.0),
        egui::pos2(1.0, 1.0),
        egui::pos2(0.0, 1.0),
    ];
    let base = mesh.vertices.len() as u32;
    for (corner, uv) in mapped.iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *corner,
            uv,
            color: egui::Color32::WHITE,
        });
    }
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    painter.add(mesh);

    ui.add_space(tokens.space(Space::Xs));
    ui.weak("where it lands — the surface does this to it, not the control");
}
