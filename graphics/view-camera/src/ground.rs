//! An orthographic camera over flat ground, z up.
//!
//! ## The model
//!
//! The camera looks at a **focus** (a point; its height stays where it was
//! put — a diorama's middle, not its floor) from an **azimuth** (radians: the
//! camera sits toward `(sin a, −cos a)` in plan, so 0 looks north from the
//! south) and an **elevation** (radians above the horizon), and sees **span**
//! metres either side of the focus up and down the screen (half the view's
//! height; a macroquad orthographic `Camera3D`'s `fovy` is twice it).
//!
//! The ground is the plane `z = ground` (0 unless set). Drags and zooms hold
//! a point of the ground under a point of the screen; in an orthographic view
//! that is exact, since moving the focus moves the ground under every point
//! of the screen by the same amount.
//!
//! ## Moving it
//!
//! [`Camera::grab`], [`Camera::drag`], [`Camera::release`] carry the ground,
//! and a release coasts on the drag's momentum. [`Camera::zoom`] eases the
//! span toward a target, holding the ground under the pointer.
//! [`Camera::orbit`] turns and tilts about the focus. [`Camera::drive`] takes
//! held keys ([`Drive`]). [`Camera::fly_to`] carries the camera to a
//! [`Place`]. [`Camera::update`] moves everything under way by a frame.
//! A [`Config`] sets its [`Limits`], which keep it in sight of the scene, and
//! whether a released drag coasts.

use crate::motion::{ease, smooth, Drive, COAST, EASE, ZOOM_EASE};
use glam::{Vec2, Vec3};
use std::f32::consts::TAU;

/// Where the camera is: what a flight blends between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub focus: Vec3,
    /// Half the view's height at the focus, metres.
    pub span: f32,
    /// Radians; the camera sits toward `(sin a, −cos a)` in plan.
    pub azimuth: f32,
    /// Radians above the horizon.
    pub elevation: f32,
}

impl Place {
    /// A place with its angles in degrees.
    pub fn degrees(focus: Vec3, span: f32, azimuth_deg: f32, elevation_deg: f32) -> Self {
        Self {
            focus,
            span,
            azimuth: azimuth_deg.to_radians(),
            elevation: elevation_deg.to_radians(),
        }
    }
}

/// How far the camera may go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min_span: f32,
    pub max_span: f32,
    /// Radians above the horizon.
    pub min_elevation: f32,
    pub max_elevation: f32,
    /// The focus stays within `radius` of `centre` in plan, so the scene
    /// cannot be dragged out of sight.
    pub centre: Vec2,
    pub radius: f32,
}

impl Limits {
    /// Room to explore a scene `radius` metres across from its middle: in
    /// to a few metres, out to see it whole twice over, from 5° to 85°.
    pub fn around(centre: Vec2, radius: f32) -> Self {
        Self {
            min_span: 1.5,
            max_span: radius.max(1.0) * 2.0,
            min_elevation: 5f32.to_radians(),
            max_elevation: 85f32.to_radians(),
            centre,
            radius,
        }
    }
}

/// How a camera behaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub limits: Limits,
    /// Whether a released drag carries on, slowing, on its momentum. Off, the
    /// ground stops where the pointer let go of it.
    pub coast: bool,
}

impl Config {
    /// [`Limits::around`], coasting.
    pub fn around(centre: Vec2, radius: f32) -> Self {
        Self {
            limits: Limits::around(centre, radius),
            coast: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Grab {
    held: Vec3,
    at: Vec2,
}

#[derive(Clone, Copy, Debug)]
struct Zoom {
    target: f32,
    anchor: Option<(Vec3, Vec2)>,
}

#[derive(Clone, Copy, Debug)]
struct Flight {
    from: Place,
    to: Place,
    t: f32,
    duration: f32,
}

/// How quickly a drag's measured velocity settles (to 1/e): smooth enough
/// that one jittery frame does not fling the view on release.
const MOMENTUM_EASE: f32 = 0.06;
/// The slowest coast worth carrying on, a share of the span a second.
const COAST_STOP: f32 = 0.02;

/// The camera, and whatever is moving it.
#[derive(Clone, Debug)]
pub struct Camera {
    place: Place,
    limits: Limits,
    coast: bool,
    ground: f32,
    /// The screen's width over its height, as last seen.
    aspect: f32,
    grab: Option<Grab>,
    /// The focus's plan velocity while dragged, smoothed: what a release
    /// coasts on.
    momentum: Vec2,
    coasting: bool,
    zoom: Option<Zoom>,
    drive: Drive,
    speed: Drive,
    flight: Option<Flight>,
}

impl Camera {
    pub fn new(place: Place, config: Config) -> Self {
        let mut camera = Self {
            place,
            limits: config.limits,
            coast: config.coast,
            ground: 0.0,
            aspect: 1.0,
            grab: None,
            momentum: Vec2::ZERO,
            coasting: false,
            zoom: None,
            drive: Drive::default(),
            speed: Drive::default(),
            flight: None,
        };
        camera.place = camera.limited(camera.place);
        camera
    }

    pub fn place(&self) -> Place {
        self.place
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// The height of the ground the drags and zooms hold.
    pub fn set_ground(&mut self, z: f32) {
        self.ground = z;
    }

    /// Unit vector from the focus toward the camera.
    pub fn toward_camera(&self) -> Vec3 {
        toward(&self.place)
    }

    /// The view's full height, metres: a macroquad orthographic `fovy`.
    pub fn view_height(&self) -> f32 {
        self.place.span * 2.0
    }

    /// Whether anything is moving the camera: a viewer that draws only on a
    /// change keeps drawing while this holds.
    pub fn moving(&self) -> bool {
        self.grab.is_some()
            || self.coasting
            || self.zoom.is_some()
            || self.flight.is_some()
            || !self.speed.is_still()
            || !self.drive.is_still()
    }

    /// The point of the ground under a point of the screen (`ndc`, −1 to 1,
    /// y up). Looking along the ground (elevation 0) it is the focus's own
    /// ground point.
    pub fn ground_at(&self, ndc: Vec2) -> Vec3 {
        ground_from(&self.place, self.aspect, self.ground, ndc)
    }

    /// Take hold of the ground under a point of the screen. Stops a flight
    /// and any coasting; a zoom under way carries on but holds nothing, since
    /// the drag holds the ground now.
    pub fn grab(&mut self, ndc: Vec2) {
        self.flight = None;
        self.coasting = false;
        self.momentum = Vec2::ZERO;
        if let Some(zoom) = &mut self.zoom {
            zoom.anchor = None;
        }
        self.grab = Some(Grab {
            held: self.ground_at(ndc),
            at: ndc,
        });
    }

    /// The held ground follows the pointer to a new point of the screen.
    pub fn drag(&mut self, ndc: Vec2) {
        if let Some(grab) = &mut self.grab {
            grab.at = ndc;
        }
    }

    /// Let go: the camera coasts on the drag's momentum, if its [`Config`]
    /// says so; else it stops where it is.
    pub fn release(&mut self) {
        if self.grab.take().is_some() {
            self.coasting = self.coast && self.momentum.length() > COAST_STOP * self.place.span;
            if !self.coasting {
                self.momentum = Vec2::ZERO;
            }
        }
    }

    pub fn grabbing(&self) -> bool {
        self.grab.is_some()
    }

    /// Zoom in by `steps` (out, negative): each step a fifth of the span,
    /// eased in, holding the ground under `ndc` (the pointer) where given,
    /// else the focus. During a drag the drag's hold is the one kept.
    pub fn zoom(&mut self, steps: f32, ndc: Option<Vec2>) {
        self.flight = None;
        let from = self.zoom.map_or(self.place.span, |z| z.target);
        let target =
            (from * (-0.2 * steps).exp()).clamp(self.limits.min_span, self.limits.max_span);
        let anchor = ndc
            .filter(|_| self.grab.is_none())
            .map(|at| (self.ground_at(at), at));
        self.zoom = Some(Zoom { target, anchor });
    }

    /// Turn the view clockwise seen from above, and tilt it toward looking
    /// straight down, radians, about the focus: it stays in the middle of the
    /// screen. (The globe's sense of a turn; a larger azimuth turns the view
    /// the other way.)
    pub fn orbit(&mut self, turn: f32, tilt: f32) {
        self.flight = None;
        self.place.azimuth = (self.place.azimuth - turn).rem_euclid(TAU);
        self.place.elevation = (self.place.elevation + tilt)
            .clamp(self.limits.min_elevation, self.limits.max_elevation);
        if let Some(grab) = &mut self.grab {
            // The view turned under the pointer: hold what is under it now.
            grab.held = ground_from(&self.place, self.aspect, self.ground, grab.at);
        }
    }

    /// The keys held this frame.
    pub fn drive(&mut self, drive: Drive) {
        if !drive.is_still() {
            self.flight = None;
        }
        self.drive = drive;
    }

    /// Fly to `to` over about `seconds`, easing in and out, the azimuth the
    /// short way round.
    pub fn fly_to(&mut self, to: Place, seconds: f32) {
        self.grab = None;
        self.coasting = false;
        self.zoom = None;
        self.flight = Some(Flight {
            from: self.place,
            to: self.limited(to),
            t: 0.0,
            duration: seconds.max(0.01),
        });
    }

    /// Jump to `to`, nothing under way.
    pub fn set_place(&mut self, to: Place) {
        self.place = self.limited(to);
        self.grab = None;
        self.coasting = false;
        self.zoom = None;
        self.flight = None;
        self.momentum = Vec2::ZERO;
    }

    /// Move everything under way by `dt` seconds, on a screen `aspect` wide
    /// over high.
    pub fn update(&mut self, dt: f32, aspect: f32) {
        self.aspect = aspect.max(1e-3);
        let dt = dt.clamp(0.0, 0.1);
        if dt <= 0.0 {
            return;
        }

        if let Some(flight) = &mut self.flight {
            flight.t += dt;
            let s = smooth(flight.t / flight.duration);
            let (from, to) = (flight.from, flight.to);
            let done = flight.t >= flight.duration;
            self.place = if done { to } else { between(&from, &to, s) };
            if done {
                self.flight = None;
            }
            return;
        }

        // Held keys, eased.
        self.speed = self.speed.toward(self.drive, ease(dt, EASE));
        let pace = 1.2 * self.place.span * dt;
        if self.speed.ahead != 0.0 || self.speed.right != 0.0 {
            let (ahead, right) = plan_axes(&self.place);
            self.place.focus +=
                (ahead * self.speed.ahead + right * self.speed.right).extend(0.0) * pace;
        }
        // `Drive`'s senses: turn clockwise, tilt toward the horizon.
        self.place.azimuth = (self.place.azimuth - self.speed.turn * 1.6 * dt).rem_euclid(TAU);
        self.place.elevation = (self.place.elevation - self.speed.tilt * 0.8 * dt)
            .clamp(self.limits.min_elevation, self.limits.max_elevation);
        if self.speed.zoom != 0.0 {
            self.place.span = (self.place.span * (-1.6 * self.speed.zoom * dt).exp())
                .clamp(self.limits.min_span, self.limits.max_span);
            if let Some(zoom) = &mut self.zoom {
                zoom.target = self.place.span;
            }
        }

        // A zoom easing to its span, the ground under the pointer held.
        if let Some(zoom) = self.zoom {
            let (now, to) = (self.place.span.ln(), zoom.target.ln());
            let next = now + (to - now) * ease(dt, ZOOM_EASE);
            let settled = (to - next).abs() < 1e-3;
            self.place.span = if settled { zoom.target } else { next.exp() };
            if let Some((held, at)) = zoom.anchor {
                self.bring(held, at);
            }
            if settled {
                self.zoom = None;
            }
        }

        // A drag holds its ground under the pointer; its velocity, smoothed,
        // is what a release coasts on.
        if let Some(grab) = self.grab {
            let before = self.place.focus;
            self.bring(grab.held, grab.at);
            let velocity = (self.place.focus - before).truncate() / dt;
            self.momentum += (velocity - self.momentum) * ease(dt, MOMENTUM_EASE);
        } else if self.coasting {
            self.place.focus += self.momentum.extend(0.0) * dt;
            self.momentum *= (-dt / COAST).exp();
            if self.momentum.length() < COAST_STOP * self.place.span {
                self.coasting = false;
                self.momentum = Vec2::ZERO;
            }
        }

        self.place = self.limited(self.place);
    }

    /// Move the focus so the ground point `held` lies under `ndc`.
    fn bring(&mut self, held: Vec3, ndc: Vec2) {
        let under = self.ground_at(ndc);
        self.place.focus.x += held.x - under.x;
        self.place.focus.y += held.y - under.y;
    }

    fn limited(&self, mut place: Place) -> Place {
        let l = &self.limits;
        place.span = place.span.clamp(l.min_span, l.max_span);
        place.elevation = place.elevation.clamp(l.min_elevation, l.max_elevation);
        let off = place.focus.truncate() - l.centre;
        if off.length() > l.radius {
            let kept = l.centre + off.normalize() * l.radius;
            place.focus.x = kept.x;
            place.focus.y = kept.y;
        }
        place
    }
}

fn toward(place: &Place) -> Vec3 {
    let (sa, ca) = place.azimuth.sin_cos();
    let (se, ce) = place.elevation.sin_cos();
    Vec3::new(sa * ce, -ca * ce, se)
}

/// The screen's right and up in the world.
fn screen_axes(place: &Place) -> (Vec3, Vec3) {
    let forward = -toward(place);
    let right = forward.cross(Vec3::Z).normalize_or_zero();
    let right = if right == Vec3::ZERO { Vec3::X } else { right };
    (right, right.cross(forward))
}

/// Up the screen and right along the ground, unit plan vectors.
fn plan_axes(place: &Place) -> (Vec2, Vec2) {
    let (sa, ca) = place.azimuth.sin_cos();
    // Into the screen is away from the camera: (−sin a, cos a).
    (Vec2::new(-sa, ca), Vec2::new(ca, sa))
}

fn ground_from(place: &Place, aspect: f32, ground: f32, ndc: Vec2) -> Vec3 {
    let (right, up) = screen_axes(place);
    let on_screen = place.focus + right * (ndc.x * place.span * aspect) + up * (ndc.y * place.span);
    let forward = -toward(place);
    if forward.z.abs() < 1e-4 {
        return Vec3::new(on_screen.x, on_screen.y, ground);
    }
    let t = (ground - on_screen.z) / forward.z;
    on_screen + forward * t
}

fn between(from: &Place, to: &Place, s: f32) -> Place {
    let turn =
        (to.azimuth - from.azimuth + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
    Place {
        focus: from.focus.lerp(to.focus, s),
        span: (from.span.ln() + (to.span.ln() - from.span.ln()) * s).exp(),
        azimuth: (from.azimuth + turn * s).rem_euclid(TAU),
        elevation: from.elevation + (to.elevation - from.elevation) * s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn camera() -> Camera {
        camera_with(Config::around(Vec2::ZERO, 40.0))
    }

    fn camera_with(config: Config) -> Camera {
        let mut c = Camera::new(
            Place::degrees(Vec3::new(0.0, 0.0, 1.5), 17.0, 45.0, 36.0),
            config,
        );
        c.update(FRAME, 1.5);
        c
    }

    #[test]
    fn without_coasting_a_released_drag_stops_where_it_let_go() {
        let mut c = camera_with(Config {
            coast: false,
            ..Config::around(Vec2::ZERO, 40.0)
        });
        c.grab(Vec2::ZERO);
        for i in 1..=20 {
            c.drag(Vec2::new(0.02 * i as f32, 0.0));
            c.update(FRAME, 1.5);
        }
        c.release();
        let at_release = c.place().focus;
        run(&mut c, 30);
        assert!(!c.moving(), "a drag let go should not coast");
        assert_eq!(c.place().focus, at_release);
    }

    fn run(c: &mut Camera, frames: usize) {
        for _ in 0..frames {
            c.update(FRAME, 1.5);
        }
    }

    #[test]
    fn the_middle_of_the_screen_is_the_ground_under_the_focus() {
        let c = camera();
        let g = c.ground_at(Vec2::ZERO);
        // Straight back along the sight from the focus, down to z = 0.
        let back = g - c.place().focus;
        assert!(g.z.abs() < 1e-5);
        assert!(
            back.normalize().dot(-c.toward_camera()) > 0.9999,
            "{back:?}"
        );
    }

    #[test]
    fn a_drag_keeps_the_ground_under_the_pointer() {
        let mut c = camera();
        let start = Vec2::new(0.3, -0.2);
        let held = c.ground_at(start);
        c.grab(start);
        for i in 1..=30 {
            let at = start + Vec2::new(-0.02, 0.015) * i as f32;
            c.drag(at);
            c.update(FRAME, 1.5);
            let under = c.ground_at(at);
            assert!(
                (under - held).length() < 1e-3,
                "frame {i}: {under:?} slipped from {held:?}"
            );
        }
    }

    #[test]
    fn a_released_drag_coasts_then_stops() {
        let mut c = camera();
        c.grab(Vec2::ZERO);
        for i in 1..=20 {
            c.drag(Vec2::new(0.02 * i as f32, 0.0));
            c.update(FRAME, 1.5);
        }
        c.release();
        let at_release = c.place().focus;
        c.update(FRAME, 1.5);
        assert!(c.moving(), "a fast drag let go should coast");
        assert_ne!(c.place().focus, at_release, "the coast moved nothing");
        run(&mut c, 300);
        assert!(!c.moving(), "still coasting five seconds later");
    }

    #[test]
    fn a_zoom_holds_the_ground_under_the_pointer() {
        let mut c = camera();
        let at = Vec2::new(-0.5, 0.4);
        let held = c.ground_at(at);
        let span = c.place().span;
        c.zoom(3.0, Some(at));
        run(&mut c, 120);
        assert!(!c.moving());
        assert!(c.place().span < span * 0.6, "zoomed to {}", c.place().span);
        assert!((c.ground_at(at) - held).length() < 1e-3);
    }

    #[test]
    fn zoom_and_elevation_stay_within_the_limits() {
        let mut c = camera();
        c.zoom(-100.0, None);
        c.orbit(0.0, 10.0);
        run(&mut c, 300);
        let l = c.limits();
        assert!((c.place().span - l.max_span).abs() < 1e-3);
        assert!((c.place().elevation - l.max_elevation).abs() < 1e-5);
    }

    #[test]
    fn the_scene_cannot_be_dragged_out_of_reach() {
        let mut c = camera();
        c.grab(Vec2::ZERO);
        for i in 1..=200 {
            c.drag(Vec2::new(0.05 * i as f32, 0.0));
            c.update(FRAME, 1.5);
        }
        let off = c.place().focus.truncate().length();
        assert!(off <= c.limits().radius + 1e-3, "{off} m out");
    }

    #[test]
    fn an_orbit_turns_about_the_focus() {
        let mut c = camera();
        let focus = c.place().focus;
        c.orbit(1.0, -0.2);
        assert_eq!(c.place().focus, focus);
    }

    #[test]
    fn a_positive_turn_turns_the_view_clockwise_from_above() {
        let mut c = camera();
        let before = -c.toward_camera().truncate();
        c.orbit(0.2, 0.0);
        let after = -c.toward_camera().truncate();
        // Clockwise seen from above (z up): the cross product points down.
        assert!(before.perp_dot(after) < 0.0, "{before:?} → {after:?}");
    }

    #[test]
    fn holding_ahead_travels_up_the_screen() {
        let mut c = camera();
        let before = c.ground_at(Vec2::ZERO);
        c.drive(Drive {
            ahead: 1.0,
            ..Drive::default()
        });
        run(&mut c, 30);
        c.drive(Drive::default());
        run(&mut c, 120);
        assert!(!c.moving(), "keys let go should come to rest");
        // What was in the middle is now below it on the screen.
        let (_, up) = screen_axes(&c.place());
        let moved = c.ground_at(Vec2::ZERO) - before;
        assert!(moved.dot(up) > 0.5, "moved {moved:?}");
    }

    #[test]
    fn a_flight_arrives_exactly() {
        let mut c = camera();
        let to = Place::degrees(Vec3::new(10.0, -5.0, 1.5), 8.0, 300.0, 60.0);
        c.fly_to(to, 1.0);
        run(&mut c, 30);
        assert!(c.moving());
        run(&mut c, 40);
        assert!(!c.moving());
        assert_eq!(c.place(), to);
    }
}
