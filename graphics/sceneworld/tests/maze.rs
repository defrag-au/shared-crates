//! Hilbert palace: corridor maze + generator dump. No raster.

use scenegraph::{transform_point, Fonts, Geometry, Kind, Materials, Scene, Vec3};
use sceneworld::{hilbert_xy, Wall, World, PALACE_ORDER, PALACE_SPAN};

const PALACE: &str = include_str!("golden/palace.sw1");

fn palace() -> World {
    World::from_dump(PALACE).unwrap()
}

fn ibm() -> Fonts {
    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    fonts
}

#[test]
fn palace_dump_roundtrip() {
    let w = palace();
    assert_eq!(w.dump(), PALACE);
    assert_eq!(w.order, Some(PALACE_ORDER));
    assert_eq!(w.span, PALACE_SPAN);
    assert_eq!(w.seed, 1);
    assert_eq!(w.prints().len(), 3);
    assert!(w.hangs().is_empty());
}

fn origin_corridor_wall(w: &World) -> Wall {
    let doors: Vec<Wall> = [Wall::N, Wall::E, Wall::S, Wall::W]
        .into_iter()
        .filter(|wall| w.corridor((0, 0), wall.neighbour((0, 0))))
        .collect();
    assert_eq!(
        doors.len(),
        1,
        "Hilbert start has one corridor door, got {doors:?}"
    );
    doors[0]
}

#[test]
fn corridor_follows_the_curve() {
    let w = palace();
    let wall = origin_corridor_wall(&w);
    let nb = wall.neighbour((0, 0));
    assert!(w.corridor((0, 0), nb));
    assert!(w.corridor(nb, (0, 0)));
    assert!(!w.corridor((0, 0), (1, 1)), "diagonal is not a corridor");
}

#[test]
fn window_stays_on_the_grid() {
    let scene = palace().emit().unwrap();
    assert!(scene.get("c0_0").is_some(), "{}", scene.dump());
    assert!(
        scene.get("gallery").is_none(),
        "palace uses stable cell names"
    );
    assert!(
        scene.get("c-1_0").is_none(),
        "no rooms off the Hilbert grid"
    );
    assert!(scene.get("floor").is_some(), "one floor for the wing");
    assert!(scene.get("c0_0-floor").is_none());
    assert!(scene.get("c0_0-plinth").is_none());
    assert!(scene.tagged("hang").next().is_some());
    let groups = scene
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, Kind::Group) && n.name.starts_with('c'))
        .count();
    assert_eq!(
        groups,
        PALACE_SPAN as usize + 1,
        "stretch is span along the curve, not a Chebyshev square"
    );
    Scene::from_dump(&scene.dump()).unwrap();
}

#[test]
fn floor_does_not_jump_on_page() {
    let mut w = palace();
    let a = w.emit().unwrap();
    let (fw, fh) = match &a.get("floor").unwrap().kind {
        Kind::Quad { w, h, .. } => (*w, *h),
        other => panic!("{other:?}"),
    };
    w.at = hilbert_xy(PALACE_ORDER, 12).unwrap();
    let b = w.emit().unwrap();
    match &b.get("floor").unwrap().kind {
        Kind::Quad { w, h, .. } => {
            assert!(
                (*w - fw).abs() < 1e-4 && (*h - fh).abs() < 1e-4,
                "floor {w}x{h} vs {fw}x{fh}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn shared_walls_are_one_quad() {
    let scene = palace().emit().unwrap();
    let mut centres: Vec<Vec3> = Vec::new();
    for n in scene.nodes() {
        if !n.tags.iter().any(|t| t == "wall") {
            continue;
        }
        if !matches!(n.kind, Kind::Quad { .. }) {
            continue;
        }
        let p = transform_point(scene.world_matrix(&n.name).unwrap(), Vec3::ZERO);
        for c in &centres {
            assert!(
                (c.x - p.x).abs() > 0.05 || (c.y - p.y).abs() > 0.05 || (c.z - p.z).abs() > 0.05,
                "coplanar twin walls {} at {p:?}",
                n.name
            );
        }
        centres.push(p);
    }
    assert!(!centres.is_empty());
}

#[test]
fn corridor_is_a_door_not_a_cutoff() {
    let w = palace();
    let wall = origin_corridor_wall(&w);
    let scene = w.emit().unwrap();
    let wname = format!("wall-{}-c0_0", wall.as_str());
    assert!(
        scene.get(&wname).is_none(),
        "corridor is a missing wall, not a cut-out, have {}",
        scene
            .get(&wname)
            .map(|n| format!("{:?}", n.kind.of()))
            .unwrap_or_default()
    );
    assert!(scene.get(&format!("{wname}-l")).is_none());
    let dir = match wall {
        Wall::N => Vec3::new(0.0, 0.0, -1.0),
        Wall::S => Vec3::new(0.0, 0.0, 1.0),
        Wall::E => Vec3::new(1.0, 0.0, 0.0),
        Wall::W => Vec3::new(-1.0, 0.0, 0.0),
    };
    let mut fonts = ibm();
    let list = scene
        .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
        .unwrap();
    let hit = list
        .hit(Vec3::new(0.0, 1.6, 0.0), dir)
        .expect("far geometry");
    let depth = hit.point.x.abs().max(hit.point.z.abs());
    assert!(
        depth > 6.0,
        "must pass the corridor door into the next room, hit {} at {:?}",
        hit.name,
        hit.point
    );
}

#[test]
fn page_hysteresis_waits_until_halfway() {
    let w = palace();
    assert!(!w.should_page((0, 0)));
    let keep = PALACE_SPAN / 2;
    let near = hilbert_xy(PALACE_ORDER, keep - 1).unwrap();
    let far = hilbert_xy(PALACE_ORDER, keep).unwrap();
    assert!(!w.should_page(near), "index {keep}-1 stays on the stretch");
    assert!(w.should_page(far), "index {keep} pages");
    assert!(!w.should_page((-1, 8)), "off the grid does not page");
}

#[test]
fn same_generator_same_slice() {
    let a = palace().emit().unwrap().dump();
    let b = palace().emit().unwrap().dump();
    assert_eq!(a, b);
}
