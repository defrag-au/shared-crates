//! `PlacementPad` — pan, size and turn a subject on a flat bench, detached from
//! wherever it is actually shown.
//!
//! [`PlacementEditor`](crate::placement::PlacementEditor) draws its control *on*
//! the subject: a frame around the art where the art is. That is right when the
//! subject is shown flat, and it is the whole trouble when it is not — a decal on
//! a curved mug is drawn through a projection, so its screen shape is not its
//! value's shape, the two agree only where the surface is nearly flat, and near a
//! silhouette they cannot agree at all.
//!
//! A pad takes the other road: show the space the value lives in, flat, and let
//! the control be exact by construction. A move is a translation there, a size is
//! a distance from the centre and a turn is a bearing about it — no projection, no
//! inversion, nothing that can compound. The subject stays in its own layer, in
//! the panel where a reader is already looking, and the surface it is destined for
//! stays a preview.
//!
//! ## What the caller owns
//!
//! The placement and the space. [`PadSpace`] says what the caller's units mean:
//! the rectangle the value lives in and the physical aspect of that rectangle, so
//! a subject whose own units are not square — a decal on a mug is `aspect` wider
//! than it is tall — is drawn undistorted. A [`SizedTexture`] may be handed over
//! to paint inside the subject; without one the subject is a tinted outline.
//!
//! ## Gestures
//!
//! The body pans, the puck past the `+u` edge sizes and turns — the same
//! vocabulary and the same arithmetic as
//! [`PlacementStyle::Puck`](crate::placement::PlacementStyle::Puck), which is
//! where the drag is actually solved. The pad owns only the two things a frame
//! cannot carry: the map between the bench and the caller's units, and the drag in
//! flight.

use egui::{
    Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, epaint::Vertex, pos2, vec2,
};

use crate::placement::{Frame, Placement, PlacementEditor, PlacementStyle};
use crate::theme::{Radius, ThemeExt};

/// How much room the pad keeps around the space for the puck and its standoff.
/// `theme-exempt`: a pointer affordance, sized like `placement`'s own puck.
const MARGIN: f32 = 26.0;

/// The space a pad shows, in the caller's own units.
///
/// `rect` is the value's own rectangle — a decal's unit square, a canvas in
/// pixels, whatever the caller computes in — and `aspect` is that rectangle's
/// physical width ÷ height, which is what keeps a subject from being drawn
/// squashed. The two are separate because a unit square on a mug is not square.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadSpace {
    pub rect: Rect,
    pub aspect: f32,
}

impl PadSpace {
    /// The unit square, `aspect` times wider than it is tall on screen.
    pub fn unit(aspect: f32) -> Self {
        Self {
            rect: Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            aspect,
        }
    }
}

/// The pad's own map: the caller's units onto the bench, and back.
///
/// Split out because it is the one piece of arithmetic here that is worth
/// pinning — the bench is a rectangle and the space is a rectangle, so this is
/// affine and exact, and everything the pad does is a statement through it.
#[derive(Clone, Copy, Debug)]
struct PadMap {
    space: Rect,
    bench: Rect,
    per_u: f32,
    per_v: f32,
}

impl PadMap {
    fn new(space: PadSpace, bench: Rect) -> Self {
        let per_u = bench.width() / space.rect.width().max(1e-6);
        let per_v = bench.height() / space.rect.height().max(1e-6);
        Self {
            space: space.rect,
            bench,
            per_u,
            per_v,
        }
    }

    /// A point of the caller's space on the bench.
    fn to_px(&self, point: Pos2) -> Pos2 {
        pos2(
            self.bench.min.x + (point.x - self.space.min.x) * self.per_u,
            self.bench.min.y + (point.y - self.space.min.y) * self.per_v,
        )
    }

    /// A point of the bench back in the caller's space.
    fn from_px(&self, point: Pos2) -> Pos2 {
        pos2(
            self.space.min.x + (point.x - self.bench.min.x) / self.per_u,
            self.space.min.y + (point.y - self.bench.min.y) / self.per_v,
        )
    }

    /// The subject's rectangle as the bench shows it.
    fn frame(&self, placement: &Placement) -> Frame {
        Frame {
            corners: placement.corners().map(|corner| self.to_px(corner)),
        }
    }
}

/// The pad's cross-frame state: the drag in flight, which the editor it delegates
/// to holds.
#[derive(Default)]
pub struct PlacementPad {
    editor: PlacementEditor,
}

impl PlacementPad {
    /// Is a gesture in flight?
    pub fn dragging(&self) -> bool {
        self.editor.dragging()
    }

    /// Draw the pad across the available width and edit `placement` from it.
    /// Returns `true` on a frame where a gesture changed the value.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        placement: &mut Placement,
        space: PadSpace,
        subject: Option<egui::load::SizedTexture>,
    ) -> bool {
        let tokens = ui.tokens();
        let width = ui.available_width();
        let bench_width = (width - 2.0 * MARGIN).max(1.0);
        let height = bench_width / space.aspect.max(1e-3) + 2.0 * MARGIN;
        let (full, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let bench = Rect::from_center_size(full.center(), vec2(bench_width, height - 2.0 * MARGIN));
        let map = PadMap::new(space, bench);
        let frame = map.frame(placement);

        // The space itself, so a reader can see where the subject sits in it.
        let painter = ui.painter().clone();
        painter.rect_filled(bench, tokens.corner(Radius::Sm), tokens.color.bg_secondary);
        painter.rect_stroke(
            bench,
            tokens.corner(Radius::Sm),
            Stroke::new(1.0_f32, tokens.color.border),
            StrokeKind::Inside,
        );
        // The middle, faintly: panning needs something to pan against.
        let guide = tokens.color.text_muted.gamma_multiply(0.25);
        painter.line_segment(
            [
                pos2(bench.center().x, bench.top()),
                pos2(bench.center().x, bench.bottom()),
            ],
            Stroke::new(1.0_f32, guide),
        );
        painter.line_segment(
            [
                pos2(bench.left(), bench.center().y),
                pos2(bench.right(), bench.center().y),
            ],
            Stroke::new(1.0_f32, guide),
        );

        // The subject: the art where the caller has it, its outline otherwise —
        // and the editor tints the shape over whatever this leaves.
        if let Some(texture) = subject {
            let mut mesh = egui::Mesh::with_texture(texture.id);
            let uv = [
                pos2(0.0, 0.0),
                pos2(1.0, 0.0),
                pos2(1.0, 1.0),
                pos2(0.0, 1.0),
            ];
            let base = mesh.vertices.len() as u32;
            for (corner, uv) in frame.corners.iter().zip(uv) {
                mesh.vertices.push(Vertex {
                    pos: *corner,
                    uv,
                    color: Color32::WHITE,
                });
            }
            mesh.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            painter.add(mesh);
        }

        // The control, in the caller's own space: the pointer is already there,
        // and always is — a pad has no coverage to fall off.
        let pointer = ui
            .ctx()
            .input(|input| input.pointer.interact_pos())
            .map(|at| map.from_px(at));
        ui.scope_builder(egui::UiBuilder::new().max_rect(full), |ui| {
            self.editor
                .show_in(ui, placement, &frame, pointer, PlacementStyle::Puck)
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(aspect: f32) -> PadMap {
        // A bench 360 wide, whose height follows the space's own aspect.
        PadMap::new(
            PadSpace::unit(aspect),
            Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 360.0 / aspect)),
        )
    }

    #[test]
    fn the_bench_and_the_space_agree() {
        let map = map(1.8);
        for point in [
            pos2(0.0, 0.0),
            pos2(1.0, 1.0),
            pos2(0.42, 0.93),
            pos2(-0.2, 0.5),
        ] {
            let back = map.from_px(map.to_px(point));
            assert!((back.x - point.x).abs() < 1e-4, "{back:?} for {point:?}");
            assert!((back.y - point.y).abs() < 1e-4, "{back:?} for {point:?}");
        }
    }

    #[test]
    fn a_square_subject_is_drawn_square() {
        // A subject that is square on the surface is `aspect` taller in the
        // caller's own units — and must come out square on the bench.
        let map = map(1.8);
        let placement = Placement {
            center: pos2(0.5, 0.5),
            size: vec2(0.3, 0.3 * 1.8),
            angle: 0.0,
        };
        let frame = map.frame(&placement);
        let drawn = frame.axis_u().length() * 2.0 / (frame.axis_v().length() * 2.0);
        assert!((drawn - 1.0).abs() < 1e-4, "drawn {drawn}");
    }

    #[test]
    fn a_turned_subject_carries_its_turn_onto_the_bench() {
        let map = map(1.0);
        let square = Placement {
            center: pos2(0.5, 0.5),
            size: vec2(0.3, 0.3),
            angle: 0.35,
        };
        let frame = map.frame(&square);
        assert!((frame.placement().angle - 0.35).abs() < 1e-4);
        assert!((frame.placement().center.x - map.to_px(pos2(0.5, 0.5)).x).abs() < 1e-4);
    }
}
