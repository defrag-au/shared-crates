//! World dump → window `sg1`. No raster.

use scenegraph::{
    mat4_inverse, transform_point, Fonts, Geometry, Kind, Materials, Prim, Scene, Vec3,
};
use sceneworld::{transplant_cam, Hang, Wall, World, PRINT};

const GALLERY_SW1: &str = include_str!("golden/gallery.sw1");
const GALLERY_SG1: &str = include_str!("../../scenegraph/tests/golden/gallery.sg1");

fn gallery() -> World {
    World::from_dump(GALLERY_SW1).unwrap()
}

/// Parent-local origin. A Label on a Slab print: `|x|≤w/2`, `|y|≤h/2`, `z>0`.
fn local_in(scene: &Scene, child: &str, parent: &str) -> Vec3 {
    let inv = mat4_inverse(scene.world_matrix(parent).unwrap()).expect("invert");
    transform_point(
        inv,
        transform_point(scene.world_matrix(child).unwrap(), Vec3::ZERO),
    )
}

fn slab_print_line(scene: &Scene, child: &str, slab: &str) -> String {
    let (w, h) = match scene.get(slab).unwrap().kind {
        Kind::Mesh {
            prim: Prim::Slab { w, h, .. },
            ..
        } => (w, h),
        _ => panic!("{slab} is not a Slab"),
    };
    let p = local_in(scene, child, slab);
    let on = p.x.abs() <= w * 0.5 + 1e-4 && p.y.abs() <= h * 0.5 + 1e-4 && p.z > 0.0;
    format!(
        "{child} on {slab} local={:.4},{:.4},{:.4} {} {w}x{h}",
        p.x,
        p.y,
        p.z,
        if on { "print" } else { "OFF" }
    )
}

#[test]
fn golden_roundtrip() {
    let w = gallery();
    assert_eq!(w.dump(), GALLERY_SW1);
    assert_eq!(w.span, 0);
    assert_eq!(w.at, (0, 0));
    assert_eq!(w.hangs().len(), 4);
}

#[test]
fn rust_hook_same_as_dump() {
    let mut w = World::new("gallery");
    w.push(
        Hang::new(
            (0, 0),
            Wall::N,
            "p-toolhead",
            "285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4:546f6f6c6865616430323230",
        )
        .title("Toolhead0220"),
    )
    .unwrap();
    w.push(
        Hang::new(
            (0, 0),
            Wall::E,
            "p-pirate",
            "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6:506972617465333236",
        )
        .title("Pirate326"),
    )
    .unwrap();
    w.push(
        Hang::new(
            (0, 0),
            Wall::W,
            "p-perp",
            "e6ba9c0ff27be029442c32533c6efd956a60d15ecb976acbb64c4de0:5065727031323633",
        )
        .title("Perp1263"),
    )
    .unwrap();
    w.push(Hang::new((1, 0), Wall::N, "p-next", "demo:next").title("Next"))
        .unwrap();
    assert_eq!(w.dump(), GALLERY_SW1);
}

#[test]
fn span_zero_emit_is_gallery_sg1() {
    let scene = gallery().emit().unwrap();
    assert_eq!(scene.dump(), GALLERY_SG1);
    Scene::from_dump(&scene.dump()).unwrap();
}

#[test]
fn labels_sit_on_the_print() {
    let scene = gallery().emit().unwrap();
    for (lab, slab) in [
        ("title-toolhead", "p-toolhead"),
        ("title-pirate", "p-pirate"),
        ("title-perp", "p-perp"),
    ] {
        let line = slab_print_line(&scene, lab, slab);
        assert!(line.contains(" print"), "{line}");
    }
}

#[test]
fn span_zero_only_origin_hangs() {
    let scene = gallery().emit().unwrap();
    let hangs: Vec<_> = scene.tagged("hang").map(|n| n.name.as_str()).collect();
    assert_eq!(hangs, ["p-toolhead", "p-pirate", "p-perp"]);
    assert!(scene.get("p-next").is_none());
    assert_eq!(scene.tagged("wall").count(), 4);
}

#[test]
fn move_at_swaps_the_slice() {
    let mut w = gallery();
    w.at = (1, 0);
    let scene = w.emit().unwrap();
    let hangs: Vec<_> = scene.tagged("hang").map(|n| n.name.as_str()).collect();
    assert_eq!(hangs, ["p-next"]);
    assert!(scene.get("p-toolhead").is_none());
    assert!(
        scene.dump().contains("gallery Group t=8,0,0"),
        "{}",
        scene.dump()
    );
    assert!(
        scene
            .dump()
            .contains("p-next Mesh Slab 1.5x1.5 rad=0 sh=Unlit m=demo:next"),
        "{}",
        scene.dump()
    );
    let line = slab_print_line(&scene, "title-next", "p-next");
    assert!(line.contains(" print"), "{line}");
    let Kind::Camera { eye, at, .. } = scene.get("cam").unwrap().kind else {
        panic!("cam");
    };
    assert!(
        eye.is_near(Vec3::new(8.0, 1.6, 2.0), 1e-4),
        "cursor-local cam, {eye:?}"
    );
    assert!(at.is_near(Vec3::new(8.0, 1.5, -1.0), 1e-4), "{at:?}");
    Scene::from_dump(&scene.dump()).unwrap();
}

#[test]
fn span_one_keeps_empty_rooms() {
    let mut w = gallery();
    w.span = 1;
    let scene = w.emit().unwrap();
    assert_eq!(scene.tagged("wall").count(), 9 * 4);
    let hangs: Vec<_> = scene.tagged("hang").map(|n| n.name.as_str()).collect();
    assert_eq!(hangs, ["p-toolhead", "p-pirate", "p-perp", "p-next"]);
    assert!(scene.get("c1_0").is_some(), "{}", scene.dump());
    assert!(scene.get("c-1_-1").is_some(), "empty cells still rooms");
    assert!(scene.get("c1_0-floor").is_some());
    assert!(scene.get("plinth").unwrap().parent.as_deref() == Some("gallery"));
    assert!(scene
        .get("c1_0")
        .unwrap()
        .xf
        .t
        .is_near(Vec3::new(8.0, 0.0, 0.0), 1e-4));
    Scene::from_dump(&scene.dump()).unwrap();
}

#[test]
fn transplant_cam_survives_a_page() {
    let mut w = gallery();
    let rest = w.emit().unwrap();
    let mut walked = rest.clone();
    {
        let cam = walked.get_mut("cam").unwrap();
        let Kind::Camera { eye, at, .. } = &mut cam.kind else {
            panic!("cam");
        };
        *eye = Vec3::new(4.1, 1.6, 0.2);
        *at = Vec3::new(5.1, 1.5, -0.8);
    }
    w.at = (1, 0);
    let mut next = w.emit().unwrap();
    transplant_cam(&walked, &mut next);
    let Kind::Camera { eye, at, .. } = next.get("cam").unwrap().kind else {
        panic!("cam");
    };
    assert!(eye.is_near(Vec3::new(4.1, 1.6, 0.2), 1e-4), "{eye:?}");
    assert!(at.is_near(Vec3::new(5.1, 1.5, -0.8), 1e-4), "{at:?}");
    assert!(next.get("p-next").is_some());
    assert!(next.get("p-toolhead").is_none());
}

#[test]
fn shared_faces_are_doors() {
    let mut w = gallery();
    w.span = 1;
    let scene = w.emit().unwrap();
    assert!(
        matches!(scene.get("wall-e").unwrap().kind, Kind::Group),
        "cursor owns east: Group + jambs"
    );
    assert!(scene.get("wall-e-l").is_some(), "{}", scene.dump());
    assert!(scene.get("wall-e-r").is_some());
    assert!(
        matches!(scene.get("wall-n").unwrap().kind, Kind::Group),
        "cursor does not own north: Group, neighbour draws jambs"
    );
    assert!(scene.get("wall-n-l").is_none(), "non-owner has no quads");
    assert!(
        matches!(scene.get("wall-s").unwrap().kind, Kind::Group),
        "south neighbour is in the window"
    );
    assert!(scene.get("wall-s-l").is_some());
    let hx = scene.get("p-pirate").unwrap().xf.t.x;
    assert!(
        hx < -1.0,
        "hang must sit on the left jamb, not in the door, t.x={hx}"
    );
    let line = slab_print_line(&scene, "title-pirate", "p-pirate");
    assert!(line.contains(" print"), "{line}");
    // Window edge stays solid — (1,1) east neighbour is outside span=1.
    assert!(
        matches!(scene.get("wall-e-c1_1").unwrap().kind, Kind::Quad { .. }),
        "perimeter wall is a Quad"
    );
    Scene::from_dump(&scene.dump()).unwrap();
}

#[test]
fn east_door_does_not_block_a_ray() {
    let mut w = gallery();
    w.span = 1;
    let scene = w.emit().unwrap();
    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    let list = scene
        .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
        .unwrap();
    let closed = gallery()
        .emit()
        .unwrap()
        .draw_list_with(&Geometry::new(), &Materials::new(), &mut fonts)
        .unwrap();
    let origin = Vec3::new(0.0, 1.6, 0.0);
    let east = Vec3::new(1.0, 0.0, 0.0);
    let through = list.hit(origin, east).expect("far wall");
    let blocked = closed.hit(origin, east).expect("solid east wall");
    assert!(
        blocked.point.x < 4.5,
        "span=0 east wall at x=4, hit x={}",
        blocked.point.x
    );
    assert!(
        through.point.x > 8.0,
        "span=1 east door must pass into the neighbour, hit {} at x={}",
        through.name,
        through.point.x
    );
}

#[test]
fn print_rest_is_sharp_one_to_one() {
    let scene = gallery().emit().unwrap();
    match &scene.get("p-toolhead").unwrap().kind {
        Kind::Mesh {
            prim: Prim::Slab { w, h, radius, .. },
            ..
        } => {
            assert!((w - PRINT).abs() < 1e-4 && (h - PRINT).abs() < 1e-4);
            assert!(radius.abs() < 1e-8);
        }
        other => panic!("{other:?}"),
    }
}
