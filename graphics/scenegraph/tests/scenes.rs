//! Classic 3D room + a card-like layout. Same kinds, no pack types.

use scenegraph::{
    mat4_inverse, transform_point, DrawList, Fonts, Geometry, Kind, Materials, Prim, Scene,
    Transform, Vec3,
};

fn room_geom() -> Geometry {
    let mut g = Geometry::new();
    g.insert_tet("tet").unwrap();
    g
}

#[test]
fn room_dump_is_golden() {
    let src = include_str!("golden/room.sg1");
    let scene = Scene::from_dump(src).unwrap();
    assert_eq!(scene.dump(), src);
    assert!(scene.get("crate").is_some());
    assert_eq!(scene.kind(scenegraph::KindOf::Mesh).count(), 3);
    let draw = scene
        .draw_list_with(&room_geom(), &Materials::new(), &mut Fonts::new())
        .unwrap();
    assert_eq!(draw.camera.name, "cam");
    assert_eq!(draw.meshes.len(), 3);
    assert_eq!(draw.meshes[0].prim, Prim::Box);
    assert_eq!(draw.meshes[1].prim, Prim::Geom);
    assert_eq!(draw.meshes[2].prim, Prim::Sphere);
    assert_eq!(draw.quads.len(), 4);
    let dd = draw.dump();
    assert!(dd.contains("mesh crate Box at=0,0.4,0"), "{dd}");
    assert!(dd.contains("mesh tet Geom"), "{dd}");
    assert!(dd.contains("mesh orb Sphere"), "{dd}");
    assert!(dd.contains("g=tet"), "{dd}");
    assert!(dd.contains("lit sun Dir"), "{dd}");
    assert!(dd.contains("lit lamp Point"), "{dd}");
}

#[test]
fn cards_are_stock_slabs() {
    let src = include_str!("golden/cards.sg1");
    let scene = Scene::from_dump(src).unwrap();
    assert_eq!(scene.dump(), src);
    let hands: Vec<_> = scene.tagged("hand").map(|n| n.name.as_str()).collect();
    assert_eq!(hands, ["a", "b"]);
    let mats = Materials::from_dump(include_str!("golden/surfaces.mt1")).unwrap();
    assert_eq!(mats.dump(), include_str!("golden/surfaces.mt1"));
    let draw = scene
        .draw_list_with(&Geometry::new(), &mats, &mut Fonts::new())
        .unwrap();
    assert_eq!(draw.quads.len(), 1);
    assert_eq!(draw.meshes.len(), 2);
    assert!(matches!(
        draw.meshes[0].prim,
        Prim::Slab { w, h, .. } if (w - 2.2).abs() < 1e-4 && (h - 3.1).abs() < 1e-4
    ));
    assert!(draw.dump().contains("mesh a Slab"), "{}", draw.dump());
    assert!(draw.dump().contains("sh=Unlit"), "{}", draw.dump());
    assert!(draw.dump().contains("m=front-a"), "{}", draw.dump());
}

#[test]
fn table_is_toolheads_hand() {
    let src = include_str!("golden/table.sg1");
    let scene = Scene::from_dump(src).unwrap();
    assert_eq!(scene.dump(), src);
    let hands: Vec<_> = scene.tagged("hand").map(|n| n.name.as_str()).collect();
    assert_eq!(hands, ["th-shredder", "th-welder", "th-core"]);
    assert_eq!(scene.tagged("energy").count(), 3);
    assert_eq!(scene.tagged("player").count(), 1);
    assert_eq!(scene.tagged("enemy").count(), 1);
    assert_eq!(scene.tagged("pile").count(), 2);
    assert_eq!(scene.tagged("button").count(), 1);
    let mats = Materials::from_dump(include_str!("golden/table.mt1")).unwrap();
    assert_eq!(mats.dump(), include_str!("golden/table.mt1"));
    match &scene.get("cam").unwrap().kind {
        Kind::Camera { eye, at, .. } => {
            assert!((eye.x - at.x).abs() < 1e-4 && (eye.y - at.y).abs() < 1e-4);
            assert!(
                eye.z - at.z > 4.0,
                "frontal +Z camera, eye={eye:?} at={at:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(scene.get("th-shredder").unwrap().xf.r.z > 0.1);
    assert!(scene.get("th-core").unwrap().xf.r.z < -0.1);
    let mut fonts = ibm();
    let draw = scene
        .draw_list_with(&Geometry::new(), &mats, &mut fonts)
        .unwrap();
    assert_eq!(draw.quads.len(), 9);
    assert_eq!(draw.meshes.len(), 11);
    assert_eq!(draw.labels.len(), 14);
    assert!(
        draw.dump().contains("mesh th-shredder Slab"),
        "{}",
        draw.dump()
    );
    assert!(draw.dump().contains("m=th-shredder"), "{}", draw.dump());
    assert!(
        draw.dump().contains("mesh magnet-mother"),
        "{}",
        draw.dump()
    );
    assert!(!draw.dump().contains("missing="), "{}", draw.dump());
    for (lab, slab) in [
        ("name-player", "body"),
        ("name-mm", "magnet-mother"),
        ("cost-shredder", "th-shredder"),
        ("name-shredder", "th-shredder"),
        ("cost-welder", "th-welder"),
        ("name-welder", "th-welder"),
        ("cost-core", "th-core"),
        ("name-core", "th-core"),
        ("draw-n", "draw"),
        ("discard-n", "discard"),
        ("end-turn-n", "end-turn"),
    ] {
        let line = slab_print_line(&scene, lab, slab);
        assert!(line.contains(" print"), "{line}");
    }
    let card = draw.meshes.iter().find(|m| m.name == "th-welder").unwrap();
    let hit = draw
        .hit(draw.camera.eye, card.centre - draw.camera.eye)
        .unwrap();
    assert_eq!(
        hit.name, "th-welder",
        "centre of the hand card, not a label"
    );
}

#[test]
fn gallery_is_one_cell() {
    let src = include_str!("golden/gallery.sg1");
    let scene = Scene::from_dump(src).unwrap();
    assert_eq!(scene.dump(), src);
    let hangs: Vec<_> = scene.tagged("hang").map(|n| n.name.as_str()).collect();
    assert_eq!(hangs, ["p-toolhead", "p-pirate", "p-perp"]);
    let walls: Vec<_> = scene.tagged("wall").map(|n| n.name.as_str()).collect();
    assert_eq!(walls, ["wall-n", "wall-e", "wall-w", "wall-s"]);
    for hang in &hangs {
        let parent = scene.get(hang).unwrap().parent.as_deref().unwrap();
        assert!(
            scene.get(parent).unwrap().tags.iter().any(|t| t == "wall"),
            "{hang} parent {parent} is not a wall"
        );
    }
    let mats = Materials::from_dump(include_str!("golden/gallery.mt1")).unwrap();
    assert_eq!(mats.dump(), include_str!("golden/gallery.mt1"));
    for (lab, slab) in [
        ("title-toolhead", "p-toolhead"),
        ("title-pirate", "p-pirate"),
        ("title-perp", "p-perp"),
    ] {
        let line = slab_print_line(&scene, lab, slab);
        assert!(line.contains(" print"), "{line}");
    }
    let mut fonts = ibm();
    let draw = scene
        .draw_list_with(&Geometry::new(), &mats, &mut fonts)
        .unwrap();
    assert!(
        draw.dump().contains("mesh p-toolhead Slab"),
        "{}",
        draw.dump()
    );
    assert!(
        draw.dump().contains(
            "m=285c0b8e91ba323da4ca083c9db837e111dafbf3143ece4d03eba8f4:546f6f6c6865616430323230"
        ),
        "{}",
        draw.dump()
    );
    assert!(draw.dump().contains("mesh plinth Box"), "{}", draw.dump());
    assert!(!draw.dump().contains("missing="), "{}", draw.dump());
    let card = draw.meshes.iter().find(|m| m.name == "p-toolhead").unwrap();
    let hit = draw
        .hit(draw.camera.eye, card.centre - draw.camera.eye)
        .unwrap();
    assert_eq!(hit.name, "p-toolhead", "gaze from cam hits the north hang");
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
        "{child} on {slab} local={:.4},{:.4},{:.4} face={w}x{h} {}\n",
        p.x,
        p.y,
        p.z,
        if on { "print" } else { "OFF" }
    )
}

#[test]
fn label_under_slab_is_off_print() {
    let mut scene = Scene::from_dump(include_str!("golden/table.sg1")).unwrap();
    scene
        .set_xf(
            "name-shredder",
            Transform {
                t: Vec3::new(0.0, -1.72, 0.04),
                ..Transform::IDENTITY
            },
        )
        .unwrap();
    let line = slab_print_line(&scene, "name-shredder", "th-shredder");
    assert!(line.contains(" OFF"), "{line}");
}

#[test]
fn room_draw_list_roundtrip_scene() {
    let scene = Scene::from_dump(include_str!("golden/room.sg1")).unwrap();
    let _: DrawList = scene
        .draw_list_with(&room_geom(), &Materials::new(), &mut Fonts::new())
        .unwrap();
}

fn ibm() -> Fonts {
    let mut f = Fonts::new();
    f.insert("ibm", scenegraph::NOTO_SANS_BOLD, Vec::new())
        .unwrap();
    f
}
