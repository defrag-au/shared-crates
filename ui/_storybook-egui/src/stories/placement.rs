//! `PlacementEditor` story — move, size and turn a subject, three ways.
//!
//! The point is the comparison. All three styles over the same subject at once,
//! so "which one do I want here" gets answered by dragging all three rather
//! than by looking at a picture of one — the same reason the knob story is a
//! bench rather than a knob.
//!
//! ## The value and the pointer are in the caller's own space
//!
//! A placement is a rectangle *in some space* — screen for the flat benches,
//! surface for the keystone one — and the editor is given the pointer's
//! position in that same space. That is why a move, a size and a turn mean
//! exactly the same thing on a surface that bends as on one that does not, and
//! why there is no read-back here worth writing: what comes out of the editor
//! IS the placement.
//!
//! ## The last bench is a surface, and it is a fixture
//!
//! Not a warp model — two lines of arithmetic that squeeze the far edge — but
//! enough to see a frame that is not a rectangle, and to feel the thing that
//! matters at the edge: past it the pointer has no position on the surface, and
//! the gesture holds instead of sliding off. A real surface is the mug's remap,
//! and the customiser is where that lives.

use egui_widgets::placement::{Frame, Placement, PlacementEditor, PlacementStyle, paint_subject};
use egui_widgets::theme::{Radius, ThemeExt};

use crate::{accent, caption, heading, muted};

/// The box each style is demonstrated in.
const BENCH: egui::Vec2 = egui::vec2(300.0, 230.0);

/// The subject's own size. Decal-shaped — wider than tall, and not square, so a
/// turn is visible at a glance.
const SUBJECT: egui::Vec2 = egui::vec2(156.0, 88.0);

/// Below this, the benches are unusable slivers and stack instead. Kept at the
/// 260pt-per-bench the three-bench layout used, for all five.
const SIDE_BY_SIDE: f32 = 1300.0;

pub struct PlacementState {
    /// One editor per style: a drag is in flight in a specific control, and
    /// switching style mid-gesture must not move a different subject.
    editors: [PlacementEditor; 5],
    /// Each subject in its bench's coordinates — the centre is an offset from
    /// the bench's top-left corner, so the layout can resize underneath it.
    subjects: [Placement; 5],
    pub style: PlacementStyle,
    pub compare: bool,
    /// The surface bench: its own editor, and the subject in *surface* units —
    /// 0..1 across the face — because that is the space it is placed in.
    surface_editor: PlacementEditor,
    surface_subject: Placement,
}

impl Default for PlacementState {
    fn default() -> Self {
        let subject = |angle: f32| Placement {
            center: egui::pos2(BENCH.x * 0.5, BENCH.y * 0.56),
            size: SUBJECT,
            angle,
        };
        Self {
            editors: Default::default(),
            subjects: [
                subject(0.0),
                subject(-0.22),
                subject(0.16),
                subject(-0.10),
                subject(0.08),
            ],
            style: PlacementStyle::Handles,
            compare: true,
            surface_editor: PlacementEditor::default(),
            surface_subject: Placement {
                center: egui::pos2(0.42, 0.55),
                size: egui::vec2(0.44, 0.34),
                angle: 0.12,
            },
        }
    }
}

pub fn show(ui: &mut egui::Ui, state: &mut PlacementState) {
    heading(ui, "Placement Editor");
    caption(
        ui,
        "A subject with a centre, a size and an angle, and a control over it that \
         edits all three. Which style is right depends on the surface: a mouse \
         wants corners, a thumb wants one compound grab it cannot miss.",
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("style").color(muted(ui)));
        for style in PlacementStyle::ALL {
            let text = egui::RichText::new(style.label()).color(if state.style == *style {
                accent(ui)
            } else {
                muted(ui)
            });
            if ui.selectable_label(state.style == *style, text).clicked() {
                state.style = *style;
            }
        }
        ui.separator();
        ui.checkbox(&mut state.compare, "compare");
        if ui.button("Reset").clicked() {
            *state = PlacementState::default();
        }
    });
    caption(
        ui,
        "The subject is stored flat and the frame is its projection, so what the \
         handles sit on is where the surface actually left the corners.",
    );
    ui.add_space(10.0);

    if !state.compare {
        let index = PlacementStyle::ALL
            .iter()
            .position(|s| *s == state.style)
            .unwrap_or(0);
        bench(ui, state, index, state.style);
    } else if ui.available_width() >= SIDE_BY_SIDE {
        ui.columns(PlacementStyle::ALL.len(), |cols| {
            for (index, col) in cols.iter_mut().enumerate() {
                bench(col, state, index, PlacementStyle::ALL[index]);
            }
        });
    } else {
        for (index, style) in PlacementStyle::ALL.iter().enumerate() {
            bench(ui, state, index, *style);
            ui.add_space(12.0);
        }
    }

    ui.add_space(14.0);
    surface_bench(ui, state);
}

/// One flat bench: the caption, the boxed field, and the value it now holds.
fn bench(ui: &mut egui::Ui, state: &mut PlacementState, index: usize, style: PlacementStyle) {
    ui.label(
        egui::RichText::new(style.label())
            .color(accent(ui))
            .strong(),
    );
    ui.label(egui::RichText::new(style.hint()).color(muted(ui)).small());
    ui.add_space(4.0);

    let frame_rect = field(ui, BENCH);
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(frame_rect));
    field.set_clip_rect(frame_rect);

    let mut subject = state.subjects[index];
    subject.center = frame_rect.min + subject.center.to_vec2();
    let frame = Frame::flat(&subject);
    paint_subject(
        &field,
        &frame,
        ui.tokens().color.accent.gamma_multiply(0.18),
    );
    // Flat: the caller's own space IS the screen, so the pointer needs no
    // translating — this is the degenerate case the editor also serves.
    let pointer = field.ctx().input(|i| i.pointer.interact_pos());
    state.editors[index].show_in(&mut field, &mut subject, &frame, pointer, style);

    // Straight back into bench space, so a resize of the storybook does not
    // leave the subject behind.
    state.subjects[index] = Placement {
        center: (subject.center - frame_rect.min).to_pos2(),
        size: subject.size,
        angle: subject.angle,
    };

    ui.add_space(4.0);
    let subject = state.subjects[index];
    ui.label(
        egui::RichText::new(format!(
            "c {:.0},{:.0} · {:.0}×{:.0} · {:.0}°",
            subject.center.x,
            subject.center.y,
            subject.size.x,
            subject.size.y,
            subject.angle.to_degrees(),
        ))
        .color(muted(ui))
        .small(),
    );
}

/// The surface bench: the same control over a field that leans away, with the
/// subject stored in the surface's own units and the frame projected from them.
fn surface_bench(ui: &mut egui::Ui, state: &mut PlacementState) {
    ui.label(
        egui::RichText::new("surface (fixture)")
            .color(accent(ui))
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A keystone, not a warp model: enough that the frame is not a \
             rectangle, and that the pointer runs out of surface at the edge.",
        )
        .color(muted(ui))
        .small(),
    );
    ui.add_space(4.0);

    let frame_rect = field(ui, egui::vec2(BENCH.x.min(ui.available_width()), BENCH.y));
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(frame_rect));
    field.set_clip_rect(frame_rect);

    let subject = state.surface_subject;
    let frame = keystone_frame(frame_rect, &subject);
    paint_subject(
        &field,
        &frame,
        ui.tokens().color.accent.gamma_multiply(0.18),
    );
    // The pointer in the *surface's* units: `None` once it is off the face,
    // which is what makes the gesture hold there.
    let pointer = field
        .ctx()
        .input(|i| i.pointer.interact_pos())
        .and_then(|p| keystone_surface(frame_rect, p));
    let mut edited = subject;
    if state.surface_editor.show_in(
        &mut field,
        &mut edited,
        &frame,
        pointer,
        PlacementStyle::Puck,
    ) {
        state.surface_subject = edited;
    }

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "surface {:.2},{:.2} · {:.2}×{:.2} · {:.0}°  (puck)",
            edited.center.x,
            edited.center.y,
            edited.size.x,
            edited.size.y,
            edited.angle.to_degrees(),
        ))
        .color(muted(ui))
        .small(),
    );
}

/// A boxed field to draw a control in, returning its rectangle.
fn field(ui: &mut egui::Ui, size: egui::Vec2) -> egui::Rect {
    let tokens = ui.tokens();
    let size = egui::vec2(size.x.min(ui.available_width()), size.y);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let corner = tokens.corner(Radius::Md);
    ui.painter()
        .rect_filled(rect, corner, tokens.color.bg_secondary);
    ui.painter().rect_stroke(
        rect,
        corner,
        tokens.geometry.border(tokens.color.border),
        egui::StrokeKind::Inside,
    );
    rect
}

/// The fixture's field: surface `0..1` across the face, the far (top) edge
/// squeezed, the near edge spread. Bilinear in the two, and invertible, which
/// is all a caller's map has to be for any of this to work.
fn keystone(rect: egui::Rect, surface: egui::Pos2) -> egui::Pos2 {
    let across = 1.0 - 0.5 * surface.y;
    rect.center()
        + egui::vec2(
            (surface.x - 0.5) * rect.width() * across,
            (surface.y - 0.5) * rect.height(),
        )
}

/// And the way back, `None` off the face — the honest answer for a pointer that
/// is not on the surface any more.
fn keystone_surface(rect: egui::Rect, screen: egui::Pos2) -> Option<egui::Pos2> {
    let y = (screen.y - rect.center().y) / rect.height() + 0.5;
    if !(0.0..=1.0).contains(&y) {
        return None;
    }
    let across = 1.0 - 0.5 * y;
    let x = (screen.x - rect.center().x) / (rect.width() * across) + 0.5;
    (0.0..=1.0).contains(&x).then(|| egui::pos2(x, y))
}

/// The subject's own rectangle, projected: its corners, in the surface's units,
/// put where the fixture's field puts them.
fn keystone_frame(rect: egui::Rect, subject: &Placement) -> Frame {
    let (sin, cos) = subject.angle.sin_cos();
    let h = subject.size * 0.5;
    let corners = [
        egui::vec2(-h.x, -h.y),
        egui::vec2(h.x, -h.y),
        egui::vec2(h.x, h.y),
        egui::vec2(-h.x, h.y),
    ]
    .map(|corner| {
        let turned = egui::vec2(
            corner.x * cos - corner.y * sin,
            corner.x * sin + corner.y * cos,
        );
        keystone(rect, subject.center + turned)
    });
    Frame { corners }
}
