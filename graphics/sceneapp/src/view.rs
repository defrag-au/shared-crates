//! The live view: where the camera is, on top of rest. A game's emit never
//! moves it, and it is never in `sg1` — the same split as `scene-play`'s
//! pane camera. Camera steps (`orbit`, `turn`, `walk`) move it through the
//! same `sceneinput` tools a pointer drives, so a replay moves the view the
//! way a hand does.

use scenegraph::{CameraParams, DrawList, Scene};
use sceneinput::{stride, Event, Session, Tool};

use crate::error::Error;

/// A camera step. Closed.
#[derive(Clone, Debug, PartialEq)]
pub enum Move {
    /// An orbit drag of `dx`,`dy` ndc around the camera's target, then a
    /// wheel of `zoom` (negative is closer).
    Orbit { dx: f32, dy: f32, zoom: f32 },
    /// A walk-tool look: `dx`,`dy` ndc of pointer travel, eye fixed.
    Turn { dx: f32, dy: f32 },
    /// A walk-tool stride: `forward` along the flattened look, `strafe` to
    /// the right, in world units, stopped short of walls.
    Walk { forward: f32, strafe: f32 },
}

/// `rest` as seen from `view` (rest's own camera when there is none).
pub fn with_view(rest: &Scene, view: Option<&CameraParams>) -> Result<Scene, Error> {
    let mut out = rest.clone();
    if let Some(v) = view {
        out.put_camera(v).map_err(|e| Error::Scene {
            path: "view".into(),
            source: e,
        })?;
    }
    Ok(out)
}

/// The view after `mv`, on `shown` (rest with the current view) and its
/// flattened `list`.
pub fn apply(shown: &Scene, list: &DrawList, mv: &Move) -> Result<CameraParams, Error> {
    let input = |e| Error::Input {
        path: "view".into(),
        source: e,
    };
    let moved = match *mv {
        Move::Orbit { dx, dy, zoom } => {
            let s = Session::new().with_tool(Tool::Orbit);
            let (scene, s, _) = s
                .step(shown, list, Event::down(0.0, 0.0, 1.0))
                .map_err(input)?;
            let (scene, s, _) = s
                .step(&scene, list, Event::motion(dx, dy, 1.0))
                .map_err(input)?;
            let (scene, s, _) = s
                .step(&scene, list, Event::up(dx, dy, 1.0))
                .map_err(input)?;
            if zoom != 0.0 {
                s.step(&scene, list, Event::wheel(dx, dy, 1.0, zoom))
                    .map_err(input)?
                    .0
            } else {
                scene
            }
        }
        Move::Turn { dx, dy } => {
            let s = Session::new().with_tool(Tool::Walk);
            let (scene, s, _) = s
                .step(shown, list, Event::down(0.0, 0.0, 1.0))
                .map_err(input)?;
            s.step(&scene, list, Event::motion(dx, dy, 1.0))
                .map_err(input)?
                .0
        }
        Move::Walk { forward, strafe } => stride(shown, list, forward, strafe),
    };
    moved.camera_params().map_err(|e| Error::Scene {
        path: "view".into(),
        source: e,
    })
}

/// A world pages its window when the eye crosses into a cell far enough
/// from the one it was emitted around: the next rest, or `None` to stay.
pub fn page(world: &mut sceneworld::World, view: &CameraParams) -> Result<Option<Scene>, Error> {
    let cell = world.cell_of(view.eye.x, view.eye.z);
    if !world.should_page(cell) {
        return Ok(None);
    }
    world.at = cell;
    world.emit().map(Some).map_err(|e| Error::World {
        path: "world.sw1".into(),
        source: e,
    })
}

/// `eye=x,y,z at=x,y,z`: what a camera step asserts.
pub fn dump(v: &CameraParams) -> String {
    let p = |a: scenegraph::Vec3| format!("{},{},{}", f(a.x), f(a.y), f(a.z));
    format!("eye={} at={}", p(v.eye), p(v.at))
}

fn f(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> Scene {
        Scene::from_dump(
            "sg1
floor Quad 8x8 r=-1.5708,0,0
wall Quad 8x3 t=0,1.5,-4
cam Camera eye=0,1.6,2 at=0,1.6,0
",
        )
        .unwrap()
    }

    fn list(s: &Scene) -> DrawList {
        s.draw_list().unwrap()
    }

    #[test]
    fn walking_forward_moves_the_eye_and_stops_short_of_the_wall() {
        let s = room();
        let v = apply(
            &s,
            &list(&s),
            &Move::Walk {
                forward: 1.0,
                strafe: 0.0,
            },
        )
        .unwrap();
        assert_eq!(dump(&v), "eye=0,1.6,1 at=0,1.6,-1");
        let far = apply(
            &s,
            &list(&s),
            &Move::Walk {
                forward: 20.0,
                strafe: 0.0,
            },
        )
        .unwrap();
        assert!(far.eye.z > -4.0, "walked through the wall: {}", dump(&far));
    }

    #[test]
    fn turning_keeps_the_eye_and_orbiting_keeps_the_target() {
        let s = room();
        let t = apply(&s, &list(&s), &Move::Turn { dx: 0.5, dy: 0.0 }).unwrap();
        assert!(t.eye.is_near(scenegraph::Vec3::new(0.0, 1.6, 2.0), 1e-5));
        assert!((t.at.x - 0.0).abs() > 0.1, "{}", dump(&t));
        let o = apply(
            &s,
            &list(&s),
            &Move::Orbit {
                dx: 0.5,
                dy: 0.0,
                zoom: 0.0,
            },
        )
        .unwrap();
        assert!(o.at.is_near(scenegraph::Vec3::new(0.0, 1.6, 0.0), 1e-5));
        assert!((o.eye.x - 0.0).abs() > 0.1, "{}", dump(&o));
    }

    #[test]
    fn a_palace_pages_when_the_eye_leaves_its_window() {
        let src = include_str!("../../sceneworld/tests/golden/palace.sw1");
        let mut world = sceneworld::World::from_dump(src).unwrap();
        let start = world.at;
        let rest = world.emit().unwrap();
        let mut view = rest.camera_params().unwrap();
        assert!(
            page(&mut world, &view).unwrap().is_none(),
            "standing still is not a page"
        );
        // Four cells along: past any window's keep, so it must page.
        let (x, z) = (world.cell * 4.0, 0.0);
        view.eye.x += x;
        view.eye.z += z;
        let paged = page(&mut world, &view).unwrap().expect("a page");
        assert_ne!(world.at, start);
        assert_ne!(paged.dump(), rest.dump());
    }

    #[test]
    fn a_view_on_rest_leaves_rest_alone() {
        let s = room();
        let v = apply(
            &s,
            &list(&s),
            &Move::Walk {
                forward: 1.0,
                strafe: 0.0,
            },
        )
        .unwrap();
        let shown = with_view(&s, Some(&v)).unwrap();
        assert_ne!(shown.dump(), s.dump());
        assert_eq!(room().dump(), s.dump());
    }
}
