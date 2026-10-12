//! `PlacementPad` story — the control that is not on the subject.
//!
//! Move, size and turn in the caller's own space, drawn flat. The bench is the
//! space; the subject sits in it; neither is a picture of a surface. That is the
//! whole point: a decal on a mug is drawn through a projection, its screen shape
//! bends with the surface, and a control drawn on *that* shape can only ever
//! approximate the value it is showing — badly, wherever the surface turns away.
//!
//! Here the control and the value are the same shape, because here they are the
//! same thing.

use egui_widgets::placement::Placement;
use egui_widgets::placement_pad::{PadSpace, PlacementPad};
use egui_widgets::theme::{Space, ThemeExt};

use crate::{caption, heading};

/// The surface's own aspect — the mug body's, near enough 1.8:1 — so the bench
/// reads like the thing the value is destined for.
const ASPECT: f32 = 1.8;

/// The widest the bench is drawn. Past this the pad stops being a panel control
/// and starts being a picture.
const WIDTH: f32 = 420.0;

pub struct PlacementPadState {
    pad: PlacementPad,
    placement: Placement,
    texture: Option<egui::TextureHandle>,
}

impl Default for PlacementPadState {
    fn default() -> Self {
        Self {
            pad: PlacementPad::default(),
            // Decal-shaped: 0.3 of the surface wide, which on a body 1.8:1 is
            // more than half its height — and not square, so a turn shows.
            placement: Placement {
                center: egui::pos2(0.42, 0.52),
                size: egui::vec2(0.3, 0.3 * ASPECT),
                angle: -0.18,
            },
            texture: None,
        }
    }
}

/// A stand-in subject: a ring, a cross and a filled corner, so a turn, a size
/// and a mirror all read at a glance. A story may invent a fixture; this one
/// says nothing about any product.
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

pub fn show(ui: &mut egui::Ui, state: &mut PlacementPadState) {
    let tokens = ui.tokens();
    let texture = state.texture.get_or_insert_with(|| {
        ui.ctx().load_texture(
            "placement-pad-subject",
            subject_image(tokens.color.text_primary, tokens.color.accent),
            egui::TextureOptions::LINEAR,
        )
    });
    let subject = egui::load::SizedTexture::new(texture.id(), texture.size_vec2());

    heading(ui, "Placement Pad");
    caption(
        ui,
        "The subject's own space, drawn flat: drag it to move, drag the puck past \
         its edge to size and turn. The control never leaves the plane the value \
         lives in, so there is nothing to invert and nothing to go wrong where a \
         surface curves.",
    );
    ui.add_space(tokens.space(Space::Sm));

    // A width for the bench, before anything else has used the row: `set_max_width`
    // cannot shrink a Ui whose content already ran the full width.
    let width = egui_widgets::viewport::fit(ui, WIDTH);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            state.pad.show(
                ui,
                &mut state.placement,
                PadSpace::unit(ASPECT),
                Some(subject),
            );
        },
    );

    ui.add_space(tokens.space(Space::Sm));
    let u = state.placement.center.x;
    let v = state.placement.center.y;
    let turn = state.placement.angle.to_degrees();
    let subject_width = state.placement.size.x;
    let subject_height = state.placement.size.y;
    ui.weak(format!(
        "u {u:.2}   v {v:.2}   size {subject_width:.2} x {subject_height:.2}   turn {turn:.0}"
    ));
}
