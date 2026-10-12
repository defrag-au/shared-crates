//! `PlacementEditor` — move, size and turn a subject, three ways.
//!
//! The spatial half of "put this where I want it". A subject has a centre, a
//! size and an angle; this draws a control over it and edits all three. Three
//! styles ship side by side because which one is right depends on the surface:
//! a mouse wants corners, a thumb wants one compound grab it cannot miss, and a
//! [puck](PlacementStyle::Puck) sits between the two.
//!
//! - [`PlacementStyle::Handles`] — four corner knobs. A corner pull sizes AND
//!   turns in one gesture (distance from the opposite corner sizes, angle
//!   around it turns); the body drags.
//! - [`PlacementStyle::Halo`] — a ring around the subject: inside moves it, the
//!   ring itself turns it, outside the ring sizes it.
//! - [`PlacementStyle::Puck`] — one puck past the subject's edge: its distance
//!   sizes, its angle turns; the body drags.
//!
//! ## The value and the pointer live in the caller's own space
//!
//! [`Placement`] is a centre, an unrotated size and an angle in whatever space
//! the caller stores the subject in, and [`PlacementEditor::show_in`] is given
//! the pointer's position **in that same space**. Everything the editor
//! computes happens there: a move is a translation, a size is a scale, a turn
//! is a rotation, and none of them has to be approximated because the surface
//! underneath bends. A caller whose subject sits on a *warped* surface — a
//! decal on a curved mug — samples the pointer's surface coordinate through its
//! own map and gets the same arithmetic it would on a plane.
//!
//! ## The frame is where the surface put it, and only the eye cares
//!
//! [`Frame`] is the subject's own rectangle as the surface actually left it —
//! four screen corners, free to shear and foreshorten — and this DRAWS and
//! HIT-TESTS that. That is the whole reason the type exists: a knob drawn on
//! the warped corner is a knob the reader can see and grab, and a control that
//! draws a rectangle over a trapezoid is a control that lies about where the
//! corners are. The frame never takes part in the arithmetic — it is a
//! projection, not a space.
//!
//! ## Past the surface there is no position, so the gesture holds
//!
//! A pointer that has left the surface — off a mug's silhouette, off the
//! wrapped side — has no coordinate in the caller's space, and that is what
//! `None` says. The editor then holds the gesture exactly where it was rather
//! than extrapolating a position nobody has: the alternative invents motion
//! from a metric that is about to fold, which is how a control walks away from
//! what it is pointing at.
//!
//! Pure: `egui` only — no I/O, no runtime bindings, so a storybook story and a
//! wasm frontend drive the same code.

use std::f32::consts::{PI, TAU};

use egui::{Color32, CursorIcon, Pos2, Rect, Stroke, Ui, Vec2, vec2};

use crate::theme::ThemeExt;

/// Grab radius for a knob or the puck. `theme-exempt`: what a fingertip or a
/// cursor needs, not what the theme says.
const GRAB: f32 = 15.0;

/// Drawn knob radius. `theme-exempt`: pointer affordance.
const HANDLE: f32 = 6.0;

/// The halo's turning band, either side of the ring. `theme-exempt`: pointer
/// affordance.
const RING: f32 = 18.0;

/// How far past the subject's `+u` edge the puck sits, on screen.
/// `theme-exempt`: pointer affordance.
const PUCK: f32 = 26.0;

/// The [`PlacementStyle::Knob`]'s drawn radius, its grab radius and how far past
/// the subject's edge it stands — all sized for a thumb rather than a cursor.
/// `theme-exempt`: pointer affordances.
const KNOB: f32 = 10.0;
const KNOB_GRAB: f32 = 24.0;
const KNOB_STANDOFF: f32 = 32.0;

/// How far, on screen, a knob grab travels before it decides between a turn
/// and a size — the same rule as the dial's rim. `theme-exempt`: a feel
/// parameter.
const KNOB_LOCK: f32 = 8.0;

/// How many segments the halo's ring is drawn (and hit-tested) in. Enough that
/// a warped ring reads as a curve; `theme-exempt`: geometry, not a themed size.
const RING_STEPS: u32 = 48;

/// Below this distance from the centre a bearing is noise, so a turn waits for
/// the pointer to move out again rather than snapping. `theme-exempt`: geometry.
const BEARING_FLOOR: f32 = 6.0;

/// The limits a gesture can scale a subject to, as a multiple of the size it
/// started at.
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 20.0;

/// A subject's placement, in the caller's own space.
///
/// On a warped surface this is the *surface* coordinate, not the screen: the
/// caller's map is the only thing that knows how the two relate, and the editor
/// never needs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub center: Pos2,
    /// Size before rotation.
    pub size: Vec2,
    /// Rotation, radians, clockwise (y grows downwards in the caller's space).
    pub angle: f32,
}

impl Placement {
    /// A subject of `size`, centred and unrotated.
    pub fn new(center: Pos2, size: Vec2) -> Self {
        Self {
            center,
            size,
            angle: 0.0,
        }
    }

    /// The rotated corners, clockwise from the top-left one.
    pub fn corners(&self) -> [Pos2; 4] {
        let (sin, cos) = self.angle.sin_cos();
        let h = self.size * 0.5;
        [
            vec2(-h.x, -h.y),
            vec2(h.x, -h.y),
            vec2(h.x, h.y),
            vec2(-h.x, h.y),
        ]
        .map(|c| self.center + vec2(c.x * cos - c.y * sin, c.x * sin + c.y * cos))
    }

    /// Is `point` inside the rotated subject?
    pub fn contains(&self, point: Pos2) -> bool {
        let local = self.in_local_frame(point);
        local.x.abs() <= self.size.x * 0.5 && local.y.abs() <= self.size.y * 0.5
    }

    /// `point` in the subject's own frame: origin at its centre, unrotated.
    fn in_local_frame(&self, point: Pos2) -> Vec2 {
        let v = point - self.center;
        let (sin, cos) = (-self.angle).sin_cos();
        vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    }
}

/// Where the subject's own rectangle landed on screen: its four corners, in the
/// order top-left, top-right, bottom-right, bottom-left **of the subject's own
/// frame**, after whatever the surface under it did.
///
/// A flat surface hands back the rectangle; a curved one shears and foreshortens
/// it, and the editor draws that. The corners are the truth; between them this
/// interpolates bilinearly, and past them it extrapolates along the same plane,
/// which is where a puck or a ring outside the subject is found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub corners: [Pos2; 4],
}

impl Frame {
    /// The frame a flat surface leaves: the placement's own rectangle.
    pub fn flat(placement: &Placement) -> Self {
        Self {
            corners: placement.corners(),
        }
    }

    /// The flat reading of a frame — the centre, the mean size and the
    /// direction of the `u` axis — for a caller that stores a rectangle and
    /// wants the frame it was drawn from. The two say the same thing until a
    /// surface bends the frame, which is the point.
    pub fn placement(&self) -> Placement {
        let u = self.axis_u();
        let v = self.axis_v();
        Placement {
            center: self.center(),
            size: vec2(u.length() * 2.0, v.length() * 2.0),
            angle: u.y.atan2(u.x),
        }
    }

    /// The screen point at `unit` — `(0, 0)` top-left to `(1, 1)` bottom-right
    /// of the subject's own rectangle, bilinear through the corners, and
    /// extrapolating past them.
    pub fn at(&self, unit: Vec2) -> Pos2 {
        let [tl, tr, br, bl] = self.corners;
        let top = tl + (tr - tl) * unit.x;
        let bottom = bl + (br - bl) * unit.x;
        top + (bottom - top) * unit.y
    }

    /// Where a point of the caller's own space sits in that unit square — the
    /// bridge between the space the editor computes in and the frame it draws.
    ///
    /// A caller painting its own furniture around the subject (a caption, a
    /// badge, a guide) wants this; so does anything that has to know where a
    /// particular piece of the subject ended up after the warp.
    pub fn unit(&self, placement: &Placement, point: Pos2) -> Vec2 {
        let local = placement.in_local_frame(point);
        vec2(
            local.x / placement.size.x.max(1e-6) + 0.5,
            local.y / placement.size.y.max(1e-6) + 0.5,
        )
    }

    /// The subject's centre — unit `(0.5, 0.5)`.
    pub fn center(&self) -> Pos2 {
        self.at(vec2(0.5, 0.5))
    }

    /// Half the subject's width, along its own `u` axis, in screen pixels.
    pub fn axis_u(&self) -> Vec2 {
        self.at(vec2(1.0, 0.5)) - self.center()
    }

    /// Half the subject's height, along its own `v` axis. The two axes are not
    /// perpendicular once a surface has sheared the subject, and that is the
    /// point of handing them over as vectors.
    pub fn axis_v(&self) -> Vec2 {
        self.at(vec2(0.5, 1.0)) - self.center()
    }

    /// The mean half-diagonal.
    pub fn radius(&self) -> f32 {
        (self.axis_u().length() + self.axis_v().length()) * 0.5
    }
}

/// How the control looks and takes gestures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlacementStyle {
    /// Four corner knobs: a corner pull sizes and turns together, the body
    /// drags.
    #[default]
    Handles,
    /// A ring around the subject: inside drags, the ring turns, outside the
    /// ring sizes.
    Halo,
    /// A single puck past the subject's edge: its distance sizes, its angle
    /// turns; the body drags.
    Puck,
    /// The puck made for a thumb. One grab does ONE thing: the first few
    /// points of travel decide between a sweep round the subject (turn) and a
    /// pull along its axis (size), and the choice holds until release. The
    /// knob stays under the pointer either way, hops to the other edge when
    /// its usual one would leave the control's area, and a press on empty
    /// space does nothing.
    Knob,
    /// Move only: the body drags and nothing else is drawn or taken. For a
    /// host that turns and sizes somewhere else — a
    /// [`TransformDial`](crate::transform_dial::TransformDial) beside the
    /// subject — so the subject itself has exactly one gesture. A press on
    /// empty space does nothing.
    Drag,
}

impl PlacementStyle {
    pub const ALL: &'static [PlacementStyle] = &[
        PlacementStyle::Handles,
        PlacementStyle::Halo,
        PlacementStyle::Puck,
        PlacementStyle::Knob,
        PlacementStyle::Drag,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PlacementStyle::Handles => "handles",
            PlacementStyle::Halo => "halo",
            PlacementStyle::Puck => "puck",
            PlacementStyle::Knob => "knob",
            PlacementStyle::Drag => "drag",
        }
    }

    /// One line on what the reader does with it — the storybook caption and any
    /// in-app legend that needs to say.
    pub fn hint(self) -> &'static str {
        match self {
            PlacementStyle::Handles => "pull a corner to size and turn, the body to move",
            PlacementStyle::Halo => "drag inside to move, the ring to turn, outside to size",
            PlacementStyle::Puck => "the puck sizes and turns, the body moves",
            PlacementStyle::Knob => {
                "drag the body to move · sweep the knob to turn, pull it to size"
            }
            PlacementStyle::Drag => "drag it to move it",
        }
    }
}

/// What a knob grab became once it had travelled far enough to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KnobAxis {
    /// Round the subject's centre.
    Turn,
    /// Along the line from the centre.
    Size,
}

/// A gesture in flight. Every length here is in the **caller's** units, where
/// the subject is a rectangle and the arithmetic is the same on any surface.
#[derive(Clone, Copy, Debug)]
enum Drag {
    /// Moving: the pointer's offset from the centre when the drag began.
    Move { grab: Vec2 },
    /// A corner pull, against the opposite corner held fixed.
    Corner {
        anchor: Pos2,
        /// The grabbed corner's offset from the anchor at the press.
        from: Vec2,
        size: Vec2,
        angle: f32,
    },
    /// The halo's ring: the pointer's bearing last frame, so the turn
    /// accumulates rather than being re-derived from a stale baseline.
    Turn { last: f32 },
    /// Outside the halo: the pointer's distance from the centre at the press.
    Grow { from: f32, size: Vec2 },
    /// The puck: the offset from the centre to the puck at the press, and the
    /// subject's size and angle then. The puck's standoff is a screen distance,
    /// so it is converted to the caller's units where the frame can say how
    /// much one of those is worth.
    Puck {
        from: Vec2,
        size: Vec2,
        angle: f32,
        standoff: f32,
    },
    /// The knob. `start` is the pointer's offset from the centre at the press
    /// and `last` the one a turn last read; `lock` and `floor` are the
    /// decision distance and the bearing floor, converted from screen points.
    Knob {
        start: Vec2,
        last: Vec2,
        axis: Option<KnobAxis>,
        lock: f32,
        floor: f32,
        size: Vec2,
        standoff: f32,
    },
}

/// Where the pointer sits relative to the halo's ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RingHit {
    Inside,
    Ring,
    Outside,
}

/// A subject's own geometry, in the caller's units, placed on screen by a
/// frame. This is what the editor needs to draw and to hit-test, computed once
/// a frame so the two cannot disagree.
struct Rig {
    /// The subject's centre on screen.
    center: Pos2,
    /// The subject's corners on screen, clockwise from the top-left.
    corners: [Pos2; 4],
    ring: Vec<Pos2>,
    /// The puck, in the caller's units and on screen.
    puck: Pos2,
    puck_screen: Pos2,
    /// The knob, on screen: past the `+u` edge, or past the `−u` edge when
    /// the rig was built flipped.
    knob_screen: Pos2,
    /// Screen pixels per caller unit, averaged over the two axes.
    per_unit: f32,
}

impl Rig {
    /// The rig with the knob on its usual `+u` edge.
    #[cfg(test)]
    fn new(placement: &Placement, frame: &Frame) -> Self {
        Self::sided(placement, frame, false)
    }

    /// The rig with the knob on the `−u` edge when `flipped`.
    fn sided(placement: &Placement, frame: &Frame, flipped: bool) -> Self {
        let unit = |point: Pos2| frame.unit(placement, point);
        let at = |point: Pos2| frame.at(unit(point));
        let half = placement.size * 0.5;
        // The subject's own units are not square on screen once a surface has
        // squeezed one axis, so anything sized in screen pixels comes back
        // through this.
        let per_unit = ((frame.axis_u().length() / half.x.max(1e-4))
            + (frame.axis_v().length() / half.y.max(1e-4)))
            * 0.5;
        let per_unit = per_unit.max(1e-4);

        // The ring is the caller-space circle through the subject's corners,
        // plus a pointer-sized standoff, drawn through the frame — so a warped
        // subject gets a warped ring, and the ring the reader sees is the ring
        // the editor tests against.
        let (sin, cos) = placement.angle.sin_cos();
        let ring_radius = half.length() + (RING * 0.5) / per_unit;
        let ring = (0..RING_STEPS)
            .map(|step| {
                let angle = step as f32 / RING_STEPS as f32 * TAU;
                let offset = vec2(angle.cos(), angle.sin()) * ring_radius;
                let turned = vec2(
                    offset.x * cos - offset.y * sin,
                    offset.x * sin + offset.y * cos,
                );
                at(placement.center + turned)
            })
            .collect();

        let standoff = PUCK / per_unit;
        let out = vec2(half.x + standoff, 0.0);
        let puck = placement.center + vec2(out.x * cos - out.y * sin, out.x * sin + out.y * cos);

        let side = if flipped { -1.0 } else { 1.0 };
        let reach = side * (half.x + KNOB_STANDOFF / per_unit);
        let knob = placement.center + vec2(reach * cos, reach * sin);

        Self {
            center: frame.center(),
            corners: placement.corners().map(at),
            ring,
            puck,
            puck_screen: at(puck),
            knob_screen: at(knob),
            per_unit,
        }
    }

    /// The puck's screen standoff, in the caller's units.
    fn standoff(&self) -> f32 {
        PUCK / self.per_unit
    }

    /// Is `screen` on the knob?
    fn on_knob(&self, screen: Pos2) -> bool {
        (screen - self.knob_screen).length() < KNOB_GRAB
    }
}

/// Should the knob stand on the `−u` edge? Only when the `+u` one would leave
/// `area` and the `−u` one would not — a knob that hopped about for less would
/// be a knob the thumb had to chase.
fn knob_flipped(placement: &Placement, frame: &Frame, area: Rect) -> bool {
    let inside = |flipped| {
        area.shrink(KNOB)
            .contains(Rig::sided(placement, frame, flipped).knob_screen)
    };
    !inside(false) && inside(true)
}

/// The editor's cross-frame state: any drag in flight, and which edge the knob
/// stands on.
#[derive(Default)]
pub struct PlacementEditor {
    drag: Option<Drag>,
    /// Held through a drag, so the knob doesn't hop out from under the thumb.
    knob_flipped: bool,
}

impl PlacementEditor {
    /// Is a gesture in flight?
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Would a press at `screen` take the style's sizing/turning handle rather
    /// than the body?
    ///
    /// For a host that draws several subjects and lets a press pick which one
    /// the editor holds: a press on another subject should select it, *unless*
    /// it landed on the held subject's handle — which can stand over a
    /// neighbour, because a puck or a corner sits on or past the subject's edge.
    pub fn on_handle(
        &self,
        placement: &Placement,
        frame: &Frame,
        screen: Pos2,
        style: PlacementStyle,
    ) -> bool {
        let rig = Rig::sided(placement, frame, self.knob_flipped);
        match style {
            PlacementStyle::Handles => rig
                .corners
                .iter()
                .any(|corner| (screen - *corner).length() < GRAB),
            PlacementStyle::Halo => ring_hit(&rig, screen) != RingHit::Inside,
            PlacementStyle::Puck => (screen - rig.puck_screen).length() < GRAB,
            PlacementStyle::Knob => rig.on_knob(screen),
            // No handle at all: the body is the whole control.
            PlacementStyle::Drag => false,
        }
    }

    /// Draw the control over a flat subject — one whose own space is the
    /// screen. Returns `true` on a frame where the gesture changed it. Claims
    /// the available rect, so the subject must live inside it.
    pub fn show(&mut self, ui: &mut Ui, placement: &mut Placement, style: PlacementStyle) -> bool {
        let frame = Frame::flat(placement);
        let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
        self.show_in(ui, placement, &frame, pointer, style)
    }

    /// Draw the control over `placement`, whose screen shape is `frame`, and
    /// edit it from the pointer's position **in the caller's own space**.
    ///
    /// `pointer` is `None` wherever that space has no answer — off a surface's
    /// coverage — and the gesture then holds rather than inventing one.
    pub fn show_in(
        &mut self,
        ui: &mut Ui,
        placement: &mut Placement,
        frame: &Frame,
        pointer: Option<Pos2>,
        style: PlacementStyle,
    ) -> bool {
        let area = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(area, egui::Sense::click_and_drag());
        let tokens = ui.tokens();
        let outline = tokens.geometry.border(tokens.color.border);
        let accent = tokens.color.accent;
        if self.drag.is_none() {
            self.knob_flipped = knob_flipped(placement, frame, area);
        }
        let rig = Rig::sided(placement, frame, self.knob_flipped);

        // The press's own position, on screen: where the gesture is decided.
        let pressed = ui.ctx().input(|input| input.pointer.press_origin());
        let mut changed = false;
        if response.drag_started()
            && let Some(p) = pressed
            && let Some(local) = pointer
            && takes(placement, &rig, p, local, style)
        {
            self.drag = Some(begin(placement, &rig, p, local, style));
        }
        if response.dragged() {
            // A gesture with no position this frame keeps the last one it had:
            // the subject stays where it is until the pointer is back on the
            // surface, rather than being dragged by a guess.
            if let (Some(drag), Some(local)) = (self.drag.as_mut(), pointer) {
                changed = advance(placement, drag, local);
            }
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(hover_cursor(&rig, style, ui));
        }
        if response.drag_stopped() {
            self.drag = None;
        }

        // The subject: a soft tint so the reader reads the shape it is about to
        // handle, then the control for the style.
        for i in 0..4 {
            ui.painter()
                .line_segment([rig.corners[i], rig.corners[(i + 1) % 4]], outline);
        }
        ui.painter().add(egui::epaint::PathShape::convex_polygon(
            rig.corners.to_vec(),
            accent.gamma_multiply(0.08),
            Stroke::NONE,
        ));

        match style {
            PlacementStyle::Handles => {
                for corner in rig.corners {
                    knob(ui, corner, accent, outline);
                }
            }
            PlacementStyle::Halo => {
                // Tinted rather than at the border's weight: the ring is the
                // only thing on screen saying "this turns", and at hairline
                // weight it reads as a stray circle.
                ui.painter().add(egui::epaint::PathShape::closed_line(
                    rig.ring.clone(),
                    Stroke::new(1.5, accent.gamma_multiply(0.55)),
                ));
                // A tick on the ring where the subject's own `+u` axis meets
                // it, so "turn me" is legible before the drag.
                let (sin, cos) = placement.angle.sin_cos();
                let half = placement.size * 0.5;
                let radius = half.length() + (RING * 0.5) / rig.per_unit;
                let tick =
                    frame.at(frame.unit(placement, placement.center + vec2(cos, sin) * radius));
                let outward = vec2(-sin, cos).normalized() * HANDLE;
                ui.painter().line_segment(
                    [tick + outward, tick - outward],
                    Stroke::new(HANDLE, accent),
                );
            }
            PlacementStyle::Puck => {
                ui.painter().line_segment(
                    [frame.center(), rig.puck_screen],
                    Stroke::new(1.0, tokens.color.border),
                );
                knob(ui, rig.puck_screen, accent, outline);
            }
            PlacementStyle::Knob => {
                let painter = ui.painter();
                let reach = rig.knob_screen - rig.center;
                let guide = Stroke::new(1.5, accent.gamma_multiply(0.45));
                // Once a grab has decided, say what it decided: the orbit a
                // turn rides, or the line a size slides along.
                match self.drag {
                    Some(Drag::Knob {
                        axis: Some(KnobAxis::Turn),
                        ..
                    }) => {
                        painter.circle_stroke(rig.center, reach.length(), guide);
                    }
                    Some(Drag::Knob {
                        axis: Some(KnobAxis::Size),
                        ..
                    }) => {
                        let direction = reach.normalized();
                        painter.line_segment(
                            [
                                rig.center + direction * KNOB,
                                rig.knob_screen + direction * KNOB_STANDOFF * 1.5,
                            ],
                            guide,
                        );
                    }
                    _ => {}
                }
                painter.line_segment(
                    [rig.center, rig.knob_screen],
                    Stroke::new(1.0, tokens.color.border),
                );
                painter.circle_filled(rig.knob_screen, KNOB, accent);
                painter.circle_stroke(rig.knob_screen, KNOB, outline);
                // A hollow centre, so the knob reads as a thing to take hold
                // of rather than a dot marking a point.
                painter.circle_filled(rig.knob_screen, KNOB * 0.35, tokens.color.bg_primary);
            }
            // The outline and tint above are the whole of it: nothing to
            // grab but the subject.
            PlacementStyle::Drag => {}
        }
        changed
    }
}

/// Does a press here start a gesture at all? The older styles take any press
/// in their area as a move. The knob takes only itself and the body, and drag
/// only the body, so a press on empty space leaves the subject where it is.
fn takes(
    placement: &Placement,
    rig: &Rig,
    screen: Pos2,
    local: Pos2,
    style: PlacementStyle,
) -> bool {
    match style {
        PlacementStyle::Knob => rig.on_knob(screen) || placement.contains(local),
        PlacementStyle::Drag => placement.contains(local),
        _ => true,
    }
}

/// A filled knob.
fn knob(ui: &Ui, at: Pos2, accent: Color32, outline: Stroke) {
    ui.painter().circle_filled(at, HANDLE, accent);
    ui.painter().circle_stroke(at, HANDLE, outline);
}

/// The shortest distance from `p` to the segment `a`–`b`.
fn to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Which side of the halo's ring the pointer is on, against the ring as drawn.
fn ring_hit(rig: &Rig, p: Pos2) -> RingHit {
    let ring = &rig.ring;
    let near = ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| to_segment(p, *a, *b))
        .fold(f32::INFINITY, f32::min);
    if near <= RING {
        return RingHit::Ring;
    }
    let inner = ring
        .iter()
        .map(|q| (*q - rig.center).length())
        .fold(f32::INFINITY, f32::min);
    if (p - rig.center).length() < inner {
        RingHit::Inside
    } else {
        RingHit::Outside
    }
}

/// The cursor the pointer's position deserves, before any drag.
///
/// There is no rotate cursor in the browser's set, so a turn reads as
/// [`CursorIcon::Move`] — "this is manipulable" — rather than pretending to be a
/// resize. Sizing gestures do get the resize cursor, because that one exists and
/// would be a lie to leave out.
fn hover_cursor(rig: &Rig, style: PlacementStyle, ui: &Ui) -> CursorIcon {
    let Some(p) = ui.ctx().pointer_hover_pos() else {
        return CursorIcon::Default;
    };
    match style {
        PlacementStyle::Handles => {
            if rig
                .corners
                .iter()
                .any(|corner| (p - *corner).length() < GRAB)
            {
                CursorIcon::ResizeNwSe
            } else {
                CursorIcon::Grab
            }
        }
        PlacementStyle::Halo => match ring_hit(rig, p) {
            RingHit::Ring => CursorIcon::Move,
            RingHit::Outside => CursorIcon::ResizeNwSe,
            RingHit::Inside => CursorIcon::Grab,
        },
        PlacementStyle::Puck => {
            if (p - rig.puck_screen).length() < GRAB {
                CursorIcon::Move
            } else {
                CursorIcon::Grab
            }
        }
        PlacementStyle::Knob => {
            if rig.on_knob(p) {
                CursorIcon::Move
            } else if contains_on_screen(rig, p) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            }
        }
        PlacementStyle::Drag => {
            if contains_on_screen(rig, p) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            }
        }
    }
}

/// Is `p` inside the subject as drawn? The frame's quad, for a hover cursor
/// that has no caller-space pointer to ask with.
fn contains_on_screen(rig: &Rig, p: Pos2) -> bool {
    let corners = rig.corners;
    let sign = |a: Pos2, b: Pos2| (b - a).x * (p - a).y - (b - a).y * (p - a).x;
    let sides: Vec<f32> = (0..4)
        .map(|i| sign(corners[i], corners[(i + 1) % 4]))
        .collect();
    sides.iter().all(|s| *s >= 0.0) || sides.iter().all(|s| *s <= 0.0)
}

/// The gesture a press begins: chosen where the reader sees it, on screen, and
/// then expressed in the caller's units for the arithmetic.
fn begin(
    placement: &Placement,
    rig: &Rig,
    screen: Pos2,
    local: Pos2,
    style: PlacementStyle,
) -> Drag {
    let corners = placement.corners();
    match style {
        PlacementStyle::Handles => {
            for (index, corner) in rig.corners.iter().enumerate() {
                if (screen - *corner).length() < GRAB {
                    let anchor = corners[(index + 2) % 4];
                    return Drag::Corner {
                        anchor,
                        from: corners[index] - anchor,
                        size: placement.size,
                        angle: placement.angle,
                    };
                }
            }
            Drag::Move {
                grab: local - placement.center,
            }
        }
        PlacementStyle::Halo => {
            let offset = local - placement.center;
            match ring_hit(rig, screen) {
                RingHit::Ring if offset.length() >= BEARING_FLOOR => Drag::Turn {
                    last: offset.y.atan2(offset.x),
                },
                RingHit::Outside => Drag::Grow {
                    from: offset.length().max(1e-6),
                    size: placement.size,
                },
                _ => Drag::Move {
                    grab: local - placement.center,
                },
            }
        }
        PlacementStyle::Puck => {
            if (screen - rig.puck_screen).length() < GRAB {
                Drag::Puck {
                    from: rig.puck - placement.center,
                    size: placement.size,
                    angle: placement.angle,
                    standoff: rig.standoff(),
                }
            } else {
                Drag::Move {
                    grab: local - placement.center,
                }
            }
        }
        PlacementStyle::Knob => {
            if rig.on_knob(screen) {
                // Everything is measured from where the press landed, not from
                // the knob's centre, so a grab off-centre on a thumb-sized knob
                // does not make the subject jump.
                let start = local - placement.center;
                Drag::Knob {
                    start,
                    last: start,
                    axis: None,
                    lock: KNOB_LOCK / rig.per_unit,
                    floor: BEARING_FLOOR / rig.per_unit,
                    size: placement.size,
                    standoff: KNOB_STANDOFF / rig.per_unit,
                }
            } else {
                Drag::Move {
                    grab: local - placement.center,
                }
            }
        }
        PlacementStyle::Drag => Drag::Move {
            grab: local - placement.center,
        },
    }
}

/// One dragged frame, from the pointer's position in the caller's units.
/// Returns whether the placement changed.
fn advance(placement: &mut Placement, drag: &mut Drag, p: Pos2) -> bool {
    match drag {
        Drag::Move { grab } => {
            placement.center = p - *grab;
            true
        }
        Drag::Corner {
            anchor,
            from,
            size,
            angle,
        } => {
            let v = p - *anchor;
            let scale = (v.length() / from.length().max(1e-6)).clamp(MIN_SCALE, MAX_SCALE);
            // The subject is a rectangle HERE, whatever the surface is doing
            // with it, so the anchored corner holds still, the centre is the
            // midpoint between it and the grabbed corner, and neither needs
            // trigonometry beyond the turn itself.
            placement.center = *anchor + v * 0.5;
            placement.size = *size * scale;
            placement.angle = *angle + shortest_turn(v.y.atan2(v.x) - from.y.atan2(from.x));
            true
        }
        Drag::Turn { last } => {
            let offset = p - placement.center;
            if offset.length() < BEARING_FLOOR {
                // Too close to the centre to read a bearing; hold the angle and
                // let the pointer come back out rather than snapping.
                return false;
            }
            let now = offset.y.atan2(offset.x);
            placement.angle += shortest_turn(now - *last);
            *last = now;
            true
        }
        Drag::Grow { from, size } => {
            let distance = (p - placement.center).length();
            placement.size = *size * (distance / from.max(1e-6)).clamp(MIN_SCALE, MAX_SCALE);
            true
        }
        Drag::Puck {
            from,
            size,
            angle,
            standoff,
        } => {
            let offset = p - placement.center;
            let half = (size.x * 0.5).max(1e-6);
            // The puck rides the standoff past the edge, so growing by `d`
            // moves it out by `d`: the pointer keeps the puck under it.
            let scale = ((offset.length() - *standoff) / half).clamp(MIN_SCALE, MAX_SCALE);
            placement.size = *size * scale;
            placement.angle =
                *angle + shortest_turn(offset.y.atan2(offset.x) - from.y.atan2(from.x));
            true
        }
        Drag::Knob {
            start,
            last,
            axis,
            lock,
            floor,
            size,
            standoff,
        } => {
            let offset = p - placement.center;
            let decided = match *axis {
                Some(decided) => decided,
                None => {
                    let moved = offset - *start;
                    if moved.length() < *lock {
                        return false;
                    }
                    // Mostly round the centre is a turn, mostly along the line
                    // from it is a size — the dial's rim rule, in the subject's
                    // own units.
                    let out = start.normalized();
                    let radial = moved.dot(out);
                    let tangential = (moved - out * radial).length();
                    let decided = if tangential >= radial.abs() {
                        KnobAxis::Turn
                    } else {
                        KnobAxis::Size
                    };
                    *axis = Some(decided);
                    decided
                }
            };
            match decided {
                KnobAxis::Turn => {
                    if offset.length() < *floor {
                        return false;
                    }
                    placement.angle += shortest_turn(bearing(offset) - bearing(*last));
                    *last = offset;
                }
                KnobAxis::Size => {
                    // Relative to where the press landed, so the knob stays
                    // under the pointer however far off its centre it was taken.
                    let from = (start.length() - *standoff).max(1e-6);
                    let to = offset.length() - *standoff;
                    placement.size = *size * (to / from).clamp(MIN_SCALE, MAX_SCALE);
                }
            }
            true
        }
    }
}

/// The bearing of `v`, clockwise on screen (y grows down).
fn bearing(v: Vec2) -> f32 {
    v.y.atan2(v.x)
}

/// The shortest turn between two bearings, in (-π, π].
fn shortest_turn(radians: f32) -> f32 {
    (radians + PI).rem_euclid(2.0 * PI) - PI
}

/// Draw a subject with the shape `frame` gives it — what a story (or a host
/// that has nothing better) paints as the thing being placed.
pub fn paint_subject(ui: &Ui, frame: &Frame, fill: Color32) {
    let tokens = ui.tokens();
    ui.painter().add(egui::epaint::PathShape::convex_polygon(
        frame.corners.to_vec(),
        fill,
        tokens.geometry.border(tokens.color.accent),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    /// A flat frame for a placement, which is what a plane gives back.
    fn rig(placement: &Placement) -> Rig {
        Rig::new(placement, &Frame::flat(placement))
    }

    /// A corner pull that drags the grabbed corner onto the anchor would
    /// collapse the subject; on the way there the anchor must not move at all.
    #[test]
    fn a_corner_pull_holds_the_opposite_corner() {
        let mut placement = Placement::new(pos2(200.0, 150.0), vec2(120.0, 80.0));
        let rig = rig(&placement);
        // The bottom-right corner, whose opposite is the top-left one.
        let mut drag = begin(
            &placement,
            &rig,
            pos2(260.0, 190.0),
            pos2(260.0, 190.0),
            PlacementStyle::Handles,
        );
        assert!(matches!(drag, Drag::Corner { .. }));

        // Pulled half again as far along the same diagonal.
        advance(&mut placement, &mut drag, pos2(320.0, 230.0));
        assert!((placement.corners()[0] - pos2(140.0, 110.0)).length() < 1e-3);
        assert!((placement.size - vec2(180.0, 120.0)).length() < 1e-3);
        assert!(placement.angle.abs() < 1e-3, "the pull did not turn it");

        // Back to where it started is the size it started at.
        advance(&mut placement, &mut drag, pos2(260.0, 190.0));
        assert!((placement.size - vec2(120.0, 80.0)).length() < 1e-3);
    }

    /// The bug the incremental shape exists to avoid: holding the pointer still
    /// must not keep turning the subject.
    #[test]
    fn a_held_pointer_stops_turning() {
        let mut placement = Placement::new(pos2(100.0, 100.0), vec2(40.0, 40.0));
        let rig = rig(&placement);
        let radius = rig
            .ring
            .first()
            .map(|r| (*r - pos2(100.0, 100.0)).length())
            .unwrap_or(0.0);
        let grab = pos2(100.0 + radius, 100.0);
        let mut drag = begin(&placement, &rig, grab, grab, PlacementStyle::Halo);
        assert!(matches!(drag, Drag::Turn { .. }));

        let quarter_turn = pos2(100.0, 100.0 + radius);
        advance(&mut placement, &mut drag, quarter_turn);
        let turned = placement.angle;
        assert!(turned > 0.0, "a quarter turn clockwise is positive");
        advance(&mut placement, &mut drag, quarter_turn);
        assert_eq!(
            placement.angle, turned,
            "the same bearing twice is one turn"
        );
        // And the next quarter puts it at a half turn, not a whole one.
        advance(&mut placement, &mut drag, pos2(100.0 - radius, 100.0));
        assert!((placement.angle - 2.0 * turned).abs() < 1e-4);
    }

    /// The puck keeps its standoff past the edge, so it lands under the pointer
    /// it was dragged to.
    #[test]
    fn the_puck_follows_the_pointer() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let rig = rig(&placement);
        let mut drag = begin(
            &placement,
            &rig,
            rig.puck_screen,
            rig.puck,
            PlacementStyle::Puck,
        );
        assert!(matches!(drag, Drag::Puck { .. }));

        // Dragged anywhere, it is dragged there: the edge reaches the pointer
        // and keeps its standoff — in the caller's units, which on a plane are
        // screen pixels.
        for target in [pos2(100.0, 30.0), pos2(40.0, -90.0), pos2(200.0, 10.0)] {
            advance(&mut placement, &mut drag, target);
            let reach = (target - placement.center).length();
            assert!(
                (placement.size.x * 0.5 - (reach - PUCK)).abs() < 1e-2,
                "the edge is not at the pointer: {} against {reach}",
                placement.size.x * 0.5,
            );
            let bearing = (target - placement.center)
                .y
                .atan2((target - placement.center).x);
            assert!((placement.angle - bearing).abs() < 1e-3);
        }
        // Straight out along the axis by half the original width doubles it.
        advance(&mut placement, &mut drag, pos2(126.0, 0.0));
        assert!((placement.size.x - 200.0).abs() < 1e-3);
    }

    /// A knob grab on a flat subject, taken at the knob's centre. On a plane
    /// the caller's units are screen pixels, so screen and local agree.
    fn knob_grab(placement: &Placement) -> Drag {
        let rig = rig(placement);
        let at = rig.knob_screen;
        assert!(takes(placement, &rig, at, at, PlacementStyle::Knob));
        let drag = begin(placement, &rig, at, at, PlacementStyle::Knob);
        assert!(matches!(drag, Drag::Knob { .. }));
        drag
    }

    #[test]
    fn a_knob_sweep_turns_and_leaves_the_size_alone() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let mut drag = knob_grab(&placement);
        let reach = 50.0 + KNOB_STANDOFF;
        // Round a quarter turn, the thumb wandering in and out as it goes.
        for (step, wobble) in [(1, 6.0), (2, -5.0), (3, 8.0), (4, -3.0), (5, 0.0)] {
            let angle = step as f32 / 5.0 * PI * 0.5;
            let radius = reach + wobble;
            advance(
                &mut placement,
                &mut drag,
                pos2(angle.cos() * radius, angle.sin() * radius),
            );
        }
        assert!(
            (placement.angle - PI * 0.5).abs() < 1e-3,
            "angle {}",
            placement.angle
        );
        assert_eq!(placement.size, vec2(100.0, 50.0));
    }

    #[test]
    fn a_knob_pull_sizes_and_leaves_the_turn_alone() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let mut drag = knob_grab(&placement);
        // Straight out, drifting a little sideways: still a pull.
        let reach = 50.0 + KNOB_STANDOFF;
        for (out, drift) in [(10.0, 2.0), (30.0, -3.0), (50.0, 4.0)] {
            advance(&mut placement, &mut drag, pos2(reach + out, drift));
        }
        assert_eq!(placement.angle, 0.0);
        // The edge followed the pointer out: it is 50 further, minus the drift.
        assert!(
            (placement.size.x - 200.0).abs() < 0.5,
            "size {}",
            placement.size.x
        );
        assert!((placement.size.y / placement.size.x - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_knob_grab_does_nothing_until_it_has_decided() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let before = placement;
        let mut drag = knob_grab(&placement);
        let reach = 50.0 + KNOB_STANDOFF;
        assert!(!advance(&mut placement, &mut drag, pos2(reach + 3.0, 3.0)));
        assert_eq!(placement, before);
    }

    #[test]
    fn a_knob_press_on_empty_space_takes_nothing() {
        let placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let rig = rig(&placement);
        let empty = pos2(0.0, 120.0);
        assert!(!takes(&placement, &rig, empty, empty, PlacementStyle::Knob));
        // The body still takes a move, and the older styles keep taking any
        // press as one.
        assert!(takes(
            &placement,
            &rig,
            pos2(10.0, 5.0),
            pos2(10.0, 5.0),
            PlacementStyle::Knob
        ));
        assert!(takes(&placement, &rig, empty, empty, PlacementStyle::Puck));
    }

    #[test]
    fn drag_only_moves_and_only_from_the_body() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let rig = rig(&placement);
        // Off the body — even where another style's handle would be — is
        // nothing at all.
        let corner = rig.corners[2];
        assert!(!takes(
            &placement,
            &rig,
            corner + vec2(4.0, 4.0),
            corner + vec2(4.0, 4.0),
            PlacementStyle::Drag
        ));
        assert!(!takes(
            &placement,
            &rig,
            rig.knob_screen,
            rig.knob_screen,
            PlacementStyle::Drag
        ));
        assert!(!PlacementEditor::default().on_handle(
            &placement,
            &Frame::flat(&placement),
            corner,
            PlacementStyle::Drag
        ));

        // On the body, a move that keeps the grab where it landed and touches
        // nothing else.
        let at = pos2(40.0, 20.0);
        assert!(takes(&placement, &rig, at, at, PlacementStyle::Drag));
        let mut drag = begin(&placement, &rig, at, at, PlacementStyle::Drag);
        advance(&mut placement, &mut drag, pos2(140.0, -30.0));
        assert_eq!(placement.center, pos2(100.0, -50.0));
        assert_eq!(placement.size, vec2(100.0, 50.0));
        assert_eq!(placement.angle, 0.0);
    }

    #[test]
    fn the_knob_hops_edges_only_when_it_must() {
        let placement = Placement::new(pos2(100.0, 100.0), vec2(80.0, 40.0));
        let frame = Frame::flat(&placement);
        // Plenty of room: the usual edge.
        let roomy = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 200.0));
        assert!(!knob_flipped(&placement, &frame, roomy));
        // The right-hand side cut off just past the subject: the other edge.
        let cramped = Rect::from_min_max(pos2(0.0, 0.0), pos2(150.0, 200.0));
        assert!(knob_flipped(&placement, &frame, cramped));
        // No room either side: stay put rather than hop for nothing.
        let boxed = Rect::from_min_max(pos2(55.0, 0.0), pos2(150.0, 200.0));
        assert!(!knob_flipped(&placement, &frame, boxed));
    }

    /// Hit-testing has to follow the subject round, or a turned decal answers to
    /// a rectangle that is not on the screen.
    #[test]
    fn contains_follows_the_rotation() {
        let mut placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 20.0));
        let across = pos2(45.0, 0.0);
        let turned = pos2(0.0, 45.0);
        assert!(placement.contains(across));
        assert!(!placement.contains(turned));

        placement.angle = PI * 0.5;
        assert!(!placement.contains(across));
        assert!(placement.contains(turned));
    }

    /// The subject's own geometry is what the arithmetic runs on, so a *warped*
    /// frame must not change a single one of those numbers: it decides where the
    /// reader grabs, and nothing else. The pointer arrives in surface
    /// coordinates and a drag means exactly what it would on a plane.
    #[test]
    fn a_warped_frame_only_changes_where_you_grab() {
        let placement = Placement::new(pos2(100.0, 50.0), vec2(60.0, 40.0));
        // A surface that squeezed the subject's far edge, and leans it.
        let warped = Frame {
            corners: [
                pos2(80.0, 20.0),
                pos2(112.0, 24.0),
                pos2(126.0, 70.0),
                pos2(66.0, 76.0),
            ],
        };
        let warped_rig = Rig::new(&placement, &warped);
        let flat_rig = rig(&placement);

        // Grabbed by the bottom-right corner *as drawn* — which on a warped
        // subject is not where the rectangle's corner would be.
        let mut warped_drag = begin(
            &placement,
            &warped_rig,
            warped_rig.corners[2],
            pos2(130.0, 70.0),
            PlacementStyle::Handles,
        );
        assert!(matches!(warped_drag, Drag::Corner { .. }));
        let mut flat_drag = begin(
            &placement,
            &flat_rig,
            flat_rig.corners[2],
            pos2(130.0, 70.0),
            PlacementStyle::Handles,
        );

        // Pulled to the same surface coordinate, both give the same subject.
        let mut from_warp = placement;
        let mut from_flat = placement;
        advance(&mut from_warp, &mut warped_drag, pos2(160.0, 90.0));
        advance(&mut from_flat, &mut flat_drag, pos2(160.0, 90.0));
        assert!((from_warp.center - from_flat.center).length() < 1e-6);
        assert!((from_warp.size - from_flat.size).length() < 1e-6);
        assert!((from_warp.angle - from_flat.angle).abs() < 1e-6);
        // And the anchored corner held, in the caller's own units.
        assert!((from_warp.corners()[0] - placement.corners()[0]).length() < 1e-3);
    }

    /// The ring is drawn through the frame, so a sheared subject gets a sheared
    /// ring rather than a circle that has stopped matching it.
    #[test]
    fn the_ring_follows_the_warp() {
        let placement = Placement::new(pos2(0.0, 0.0), vec2(100.0, 60.0));
        let warped = Frame {
            corners: [
                pos2(-40.0, -20.0),
                pos2(40.0, -30.0),
                pos2(70.0, 40.0),
                pos2(-70.0, 30.0),
            ],
        };
        let rig = Rig::new(&placement, &warped);
        let center = warped.center();
        let spread = rig
            .ring
            .iter()
            .map(|p| (*p - center).length())
            .fold((f32::INFINITY, 0.0_f32), |(lo, hi), d| {
                (lo.min(d), hi.max(d))
            });
        assert!(
            spread.1 - spread.0 > 1.0,
            "a flat ring on a sheared subject is an egg, and this one is {} across",
            spread.1 - spread.0,
        );
    }

    #[test]
    fn shortest_turn_takes_the_near_way_round() {
        assert!((shortest_turn(0.1) - 0.1).abs() < 1e-6);
        let long = 2.0 * PI - 0.1;
        assert!((shortest_turn(long) + 0.1).abs() < 1e-6);
    }
}
